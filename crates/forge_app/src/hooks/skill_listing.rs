//! Per-turn skill catalog delivery via `<system_reminder>` messages.
//!
//! This module implements Claude Code's approach to making skills discoverable
//! by the LLM: on every request (via the `on_request` lifecycle hook), it
//! injects a lightweight catalog of available skills as a user-role
//! `<system_reminder>` message. The model uses this catalog to decide when to
//! invoke the `skill_fetch` tool to load a skill's full content.
//!
//! # Design goals
//!
//! - **Per-turn delivery.** Unlike the legacy partial which was statically
//!   rendered into `forge.md`'s system prompt, this handler fires for *every*
//!   request on *every* agent, so Sage and Muse (and any user-defined agent)
//!   get the skill catalog without having to copy a Handlebars partial into
//!   their prompt templates.
//! - **Delta caching.** Once a skill has been announced to a given agent in a
//!   given conversation, it is not re-listed on subsequent turns unless its
//!   description changes. This mirrors Claude Code's `sentSkillNames` cache
//!   (`claude-code/src/utils/attachments.ts:2607-2635`). New skills discovered
//!   mid-session (e.g. created via the `create-skill` workflow) are surfaced on
//!   the next turn automatically because [`ForgeSkillFetch`] exposes
//!   [`invalidate_cache`](crate::SkillFetchService::invalidate_cache).
//! - **Budget-aware formatting.** The catalog is capped at a small fraction of
//!   the model's context window (default ≈ 1%, mirroring
//!   `claude-code/src/tools/SkillTool/prompt.ts:70-171`) so listing hundreds of
//!   skills does not crowd out the user's own prompt.
//! - **Inert by default.** When the conversation has no context (pre-prompt
//!   state) or no skills are available the handler does nothing and the
//!   `ContextMessage` queue is left untouched — regressions are impossible for
//!   agents that previously worked without this hook.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use forge_domain::{
    AgentId, ContextMessage, Conversation, ConversationId, EventData, EventHandle, RequestPayload,
    Skill,
};
use forge_template::Element;
use tokio::sync::Mutex;
use tracing::{debug, warn};

use crate::SkillFetchService;

/// Default fraction of the context window reserved for the skill catalog.
///
/// Matches Claude Code's `SKILL_LISTING_BUDGET_PCT = 0.01` from
/// `claude-code/src/tools/SkillTool/prompt.ts:72`.
pub const DEFAULT_BUDGET_FRACTION: f64 = 0.01;

/// Fallback context window (in tokens) used when the caller cannot supply an
/// accurate value from model metadata.
///
/// 200k approximates the smallest commonly available frontier context
/// window (Claude Sonnet 4, GPT-5) and keeps the budget conservative.
pub const DEFAULT_CONTEXT_TOKENS: u64 = 200_000;

/// Rough character-to-token ratio used when converting between bytes and
/// tokens for budget accounting. Matches the 4-chars-per-token heuristic used
/// elsewhere in Forge (`ContextMessage::token_count_approx`).
const CHARS_PER_TOKEN: usize = 4;

/// Minimum number of skills to show in a single turn even if the budget is
/// tight. Guarantees that *something* is surfaced to the LLM when skills exist.
const MIN_SKILLS_PER_TURN: usize = 1;

/// Lightweight catalog entry derived from a [`Skill`].
///
/// Only the fields needed for listing (name + description) are kept. The full
/// skill body is loaded lazily on demand via `skill_fetch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillListing {
    /// Skill identifier as it should be passed to `skill_fetch`.
    pub name: String,
    /// One-line description shown to the LLM in the catalog.
    pub description: String,
}

impl SkillListing {
    /// Creates a new listing entry from raw parts.
    #[allow(dead_code)]
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self { name: name.into(), description: description.into() }
    }
}

impl From<&Skill> for SkillListing {
    fn from(skill: &Skill) -> Self {
        Self {
            name: skill.name.clone(),
            description: skill.description.clone(),
        }
    }
}

/// Formats a set of [`SkillListing`] entries into a catalog string, keeping
/// the total size under a token budget.
///
/// The format is deliberately simple and close to Claude Code's
/// `formatCommandsWithinBudget`:
///
/// ```text
/// - name: description
/// - another-skill: its description
/// ```
///
/// # Budget handling
///
/// `budget_tokens` is converted to a rough character budget using
/// [`CHARS_PER_TOKEN`]. Entries are added in the supplied order until the
/// budget is exhausted; at that point a summary footer noting how many
/// entries were omitted is appended if there is room.
///
/// If the budget is tight, at least [`MIN_SKILLS_PER_TURN`] entries are
/// always emitted so the LLM sees *something* — otherwise the catalog would
/// be silently empty and the reminder message would carry no information.
///
/// Returns `None` when `skills` is empty (the caller should not inject a
/// reminder at all in that case).
pub fn format_skills_within_budget(skills: &[SkillListing], budget_tokens: u64) -> Option<String> {
    if skills.is_empty() {
        return None;
    }

    let budget_chars = (budget_tokens as usize).saturating_mul(CHARS_PER_TOKEN);

    let mut lines = Vec::with_capacity(skills.len());
    let mut used_chars: usize = 0;
    let mut dropped: usize = 0;

    for (idx, skill) in skills.iter().enumerate() {
        let line = format_line(skill);
        let line_len = line.len() + 1; // + newline

        // Always admit the first MIN_SKILLS_PER_TURN entries so the catalog
        // never ends up empty when some skills exist.
        let is_minimum = idx < MIN_SKILLS_PER_TURN;

        if !is_minimum && used_chars.saturating_add(line_len) > budget_chars {
            dropped = skills.len() - idx;
            break;
        }

        used_chars = used_chars.saturating_add(line_len);
        lines.push(line);
    }

    if dropped > 0 {
        lines.push(format!(
            "- … and {dropped} more skills omitted (budget exceeded; use skill_fetch to list them)"
        ));
    }

    Some(lines.join("\n"))
}

fn format_line(skill: &SkillListing) -> String {
    // Collapse description whitespace so multi-line summaries don't break the
    // single-line list format.
    let description: String = skill
        .description
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if description.is_empty() {
        format!("- {}", skill.name)
    } else {
        format!("- {}: {}", skill.name, description)
    }
}

/// Wraps a formatted skill catalog in the final `<system_reminder>` envelope
/// sent to the LLM.
///
/// The wording mirrors Claude Code's `attachments.ts:875`:
/// it tells the model *that* skills exist and *how* to invoke them (via the
/// `skill_fetch` tool) without burning tokens duplicating the tool's own
/// description.
pub fn build_skill_reminder(catalog: &str) -> String {
    let body = format!(
        "The following skills are available for use with the skill_fetch tool. Each entry shows the skill name and a one-line description. Call skill_fetch with the skill name to load its full content before attempting the task.\n\n{catalog}"
    );
    Element::new("system_reminder").cdata(body).render()
}

/// Per-conversation / per-agent delta cache recording which skills have
/// already been announced to the LLM.
///
/// The key is `(ConversationId, AgentId)` because each agent in a multi-agent
/// conversation maintains its own context stream and must be informed
/// independently.
#[derive(Debug, Default)]
struct DeltaCache {
    sent: Mutex<HashMap<(ConversationId, AgentId), HashSet<String>>>,
}

impl DeltaCache {
    /// Returns the subset of `skills` that has not yet been announced to the
    /// given conversation/agent pair, and records the complete set as sent.
    ///
    /// The returned list preserves the ordering of `skills`.
    async fn delta(
        &self,
        conversation_id: ConversationId,
        agent_id: AgentId,
        skills: &[SkillListing],
    ) -> Vec<SkillListing> {
        let mut guard = self.sent.lock().await;
        let seen = guard.entry((conversation_id, agent_id)).or_default();

        let mut delta = Vec::new();
        for skill in skills {
            if seen.insert(skill.name.clone()) {
                delta.push(skill.clone());
            }
        }
        delta
    }

    /// Forgets all send history for a conversation. Invoked during
    /// `SessionEnd` to prevent the cache from growing unbounded across
    /// restart / resume cycles. (Not wired yet in Phase 0; exposed for future
    /// use.)
    #[allow(dead_code)]
    async fn forget(&self, conversation_id: ConversationId) {
        let mut guard = self.sent.lock().await;
        guard.retain(|(conv, _), _| *conv != conversation_id);
    }
}

/// Lifecycle hook that injects a `<system_reminder>` skill catalog before
/// every LLM request.
///
/// This is wired as part of the `on_request` hook chain in
/// [`ForgeApp::chat`](crate::app::ForgeApp::chat) and runs after existing
/// handlers (e.g. `DoomLoopDetector`).
///
/// # Lifecycle
///
/// On each invocation the handler:
/// 1. Loads the current list of skills from [`SkillFetchService::list_skills`]
///    (which goes through an internal cache).
/// 2. Computes the *delta* against what has already been announced to the
///    `(conversation_id, agent_id)` pair.
/// 3. If the delta is non-empty, formats it under the budget and appends a
///    single `ContextMessage::system_reminder` to `conversation.context`.
///
/// # Error handling
///
/// Skill-listing failures are logged at `warn` level and treated as a no-op
/// so that a transient repository error never breaks the main request flow.
pub struct SkillListingHandler<S> {
    service: Arc<S>,
    cache: Arc<DeltaCache>,
    budget_fraction: f64,
    context_tokens: u64,
}

impl<S> SkillListingHandler<S> {
    /// Creates a new handler with default budget settings.
    pub fn new(service: Arc<S>) -> Self {
        Self {
            service,
            cache: Arc::new(DeltaCache::default()),
            budget_fraction: DEFAULT_BUDGET_FRACTION,
            context_tokens: DEFAULT_CONTEXT_TOKENS,
        }
    }

    /// Overrides the fraction of the context window used for the catalog.
    /// Primarily useful for tests.
    #[allow(dead_code)]
    pub fn budget_fraction(mut self, fraction: f64) -> Self {
        self.budget_fraction = fraction;
        self
    }

    /// Overrides the assumed context window (in tokens). Primarily useful for
    /// tests and for wiring per-model limits in the future.
    pub fn context_tokens(mut self, tokens: u64) -> Self {
        self.context_tokens = tokens;
        self
    }

    fn budget_tokens(&self) -> u64 {
        let raw = (self.context_tokens as f64 * self.budget_fraction).floor();
        raw.max(0.0) as u64
    }
}

#[async_trait]
impl<S> EventHandle<EventData<RequestPayload>> for SkillListingHandler<S>
where
    S: SkillFetchService + Send + Sync + 'static,
{
    async fn handle(
        &self,
        event: &EventData<RequestPayload>,
        conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        // Load skills. A repository failure must NOT break the request, so
        // we downgrade errors to warnings and bail out early.
        let skills = match self.service.list_skills().await {
            Ok(skills) => skills,
            Err(err) => {
                warn!(
                    agent_id = %event.agent.id,
                    error = %err,
                    "Failed to load skills for <system_reminder> catalog; skipping"
                );
                return Ok(());
            }
        };

        if skills.is_empty() {
            return Ok(());
        }

        let listings: Vec<SkillListing> = skills.iter().map(SkillListing::from).collect();

        let delta = self
            .cache
            .delta(conversation.id, event.agent.id.clone(), &listings)
            .await;

        if delta.is_empty() {
            debug!(
                agent_id = %event.agent.id,
                "Skill catalog unchanged since previous turn; skipping reminder"
            );
            return Ok(());
        }

        let Some(catalog) = format_skills_within_budget(&delta, self.budget_tokens()) else {
            return Ok(());
        };

        let Some(context) = conversation.context.as_mut() else {
            // Conversation has no context yet (e.g. the very first system
            // prompt has not been set). Nothing we can append to.
            debug!(
                agent_id = %event.agent.id,
                "Conversation context not initialized; skipping skill reminder"
            );
            return Ok(());
        };

        let reminder = build_skill_reminder(&catalog);
        context
            .messages
            .push(ContextMessage::system_reminder(reminder, None).into());

        debug!(
            agent_id = %event.agent.id,
            request_count = event.payload.request_count,
            announced = delta.len(),
            total = listings.len(),
            "Injected <system_reminder> skill catalog"
        );

        Ok(())
    }
}

// ============================================================================
// Cache invalidation handler
// ============================================================================

/// Lifecycle hook that invalidates the [`SkillFetchService`] cache when a
/// tool call writes to or removes a `SKILL.md` file anywhere under a
/// `skills/` directory.
///
/// This enables mid-session skill discovery: when the user runs the
/// `create-skill` workflow (which uses the standard `write`/`patch` tools to
/// author a new `SKILL.md`), the next request will repopulate the cache and
/// [`SkillListingHandler`] will announce the new skill to the LLM on the
/// following turn.
///
/// Matches Claude Code's behavior in
/// `claude-code/src/tools/SkillTool/loader.ts`, which invalidates its in-memory
/// skill cache whenever a skill file is mutated.
///
/// # Handled tools
///
/// - `write` / `Write` (`FSWrite.file_path`)
/// - `patch` / `Patch` (`FSPatch.file_path`)
/// - `multi_patch` / `MultiPatch` (`FSMultiPatch.file_path`)
/// - `remove` / `Remove` (`FSRemove.path`)
///
/// Tool names are matched case-insensitively in snake_case form to handle
/// the normalization performed by [`crate::normalize_tool_name`].
pub struct SkillCacheInvalidator<S: SkillFetchService + ?Sized> {
    service: Arc<S>,
}

impl<S: SkillFetchService + ?Sized> SkillCacheInvalidator<S> {
    /// Creates a new cache invalidator backed by the given skill service.
    pub fn new(service: Arc<S>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl<S: SkillFetchService + ?Sized + Send + Sync>
    EventHandle<EventData<forge_domain::ToolcallEndPayload>> for SkillCacheInvalidator<S>
{
    async fn handle(
        &self,
        event: &EventData<forge_domain::ToolcallEndPayload>,
        _conversation: &mut Conversation,
    ) -> anyhow::Result<()> {
        let tool_call = &event.payload.tool_call;

        // Only act on filesystem-mutating tools.
        if !is_fs_mutation_tool(tool_call.name.as_str()) {
            return Ok(());
        }

        // Extract the target path from arguments.
        let Some(path) = extract_fs_target_path(tool_call) else {
            return Ok(());
        };

        // Only invalidate when the path looks like a skill file.
        if !is_skill_file_path(&path) {
            return Ok(());
        }

        debug!(
            tool = %tool_call.name.as_str(),
            path = %path,
            "Detected skill file mutation; invalidating skill cache"
        );

        self.service.invalidate_cache().await;

        Ok(())
    }
}

/// Returns `true` if `tool_name` (in snake_case) is one of the filesystem
/// mutation tools we care about.
fn is_fs_mutation_tool(tool_name: &str) -> bool {
    matches!(tool_name, "write" | "patch" | "multi_patch" | "remove")
}

/// Extracts the file path that a filesystem mutation tool is targeting.
///
/// - `write` / `patch` / `multi_patch` use `file_path` (with `path` alias).
/// - `remove` uses `path`.
///
/// Returns `None` if arguments cannot be parsed or the expected field is
/// missing.
fn extract_fs_target_path(tool_call: &forge_domain::ToolCallFull) -> Option<String> {
    let value = tool_call.arguments.parse().ok()?;
    let obj = value.as_object()?;

    // Try file_path first (write, patch, multi_patch), then path (remove, or
    // legacy alias).
    if let Some(v) = obj.get("file_path").and_then(|v| v.as_str()) {
        return Some(v.to_string());
    }
    if let Some(v) = obj.get("path").and_then(|v| v.as_str()) {
        return Some(v.to_string());
    }
    None
}

/// Returns `true` if `path` looks like a `SKILL.md` file living under a
/// `skills/` directory. Comparison is case-sensitive for `SKILL.md`
/// (mirroring the on-disk convention) but accepts both `/` and `\` as
/// separators.
fn is_skill_file_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");

    // Must end with `/SKILL.md` (avoid matching `SKILL.md` in the repo root).
    if !normalized.ends_with("/SKILL.md") {
        return false;
    }

    // Must contain a `/skills/` segment somewhere before the filename.
    normalized.contains("/skills/")
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_domain::{
        Agent, AgentId, Context, Conversation, ConversationId, EventData, EventHandle, ModelId,
        ProviderId, RequestPayload, Skill,
    };
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::SkillFetchService;

    // --- Budget formatter -------------------------------------------------

    #[test]
    fn test_format_single_skill() {
        let fixture = vec![SkillListing::new("pdf", "Handle PDF files")];
        let actual = format_skills_within_budget(&fixture, 1_000).unwrap();
        let expected = "- pdf: Handle PDF files";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_multiple_skills_sorted_by_input_order() {
        let fixture = vec![
            SkillListing::new("b-skill", "B"),
            SkillListing::new("a-skill", "A"),
        ];
        let actual = format_skills_within_budget(&fixture, 1_000).unwrap();
        let expected = "- b-skill: B\n- a-skill: A";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_collapses_multiline_descriptions() {
        let fixture = vec![SkillListing::new(
            "pdf",
            "Handle PDF\n  files\n  with   embedded fonts",
        )];
        let actual = format_skills_within_budget(&fixture, 1_000).unwrap();
        let expected = "- pdf: Handle PDF files with embedded fonts";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_format_empty_returns_none() {
        let fixture: Vec<SkillListing> = vec![];
        let actual = format_skills_within_budget(&fixture, 1_000);
        assert!(actual.is_none());
    }

    #[test]
    fn test_format_budget_truncation_keeps_minimum() {
        // Budget of 2 tokens = 8 chars, way below any single entry.
        // The formatter must still emit at least MIN_SKILLS_PER_TURN skills
        // and mark the rest as dropped.
        let fixture = vec![
            SkillListing::new("a-skill", "descriptive text here"),
            SkillListing::new("b-skill", "another description"),
            SkillListing::new("c-skill", "yet another one"),
        ];
        let actual = format_skills_within_budget(&fixture, 2).unwrap();
        assert!(
            actual.contains("a-skill"),
            "minimum skill not present: {actual}"
        );
        assert!(
            actual.contains("2 more skills omitted"),
            "dropped-footer not present: {actual}"
        );
    }

    #[test]
    fn test_format_missing_description() {
        let fixture = vec![SkillListing::new("bare", "")];
        let actual = format_skills_within_budget(&fixture, 1_000).unwrap();
        let expected = "- bare";
        assert_eq!(actual, expected);
    }

    // --- Reminder envelope ------------------------------------------------

    #[test]
    fn test_build_skill_reminder_wraps_catalog() {
        let catalog = "- pdf: Handle PDF files";
        let actual = build_skill_reminder(catalog);
        assert!(actual.contains("<system_reminder>"));
        assert!(actual.contains("</system_reminder>"));
        assert!(actual.contains("skill_fetch"));
        assert!(actual.contains(catalog));
    }

    // --- Delta cache ------------------------------------------------------

    #[tokio::test]
    async fn test_delta_cache_first_call_returns_all() {
        let cache = DeltaCache::default();
        let conv = ConversationId::generate();
        let agent = AgentId::new("forge");
        let skills = vec![SkillListing::new("a", "A"), SkillListing::new("b", "B")];

        let actual = cache.delta(conv, agent, &skills).await;

        assert_eq!(actual, skills);
    }

    #[tokio::test]
    async fn test_delta_cache_repeat_call_returns_empty() {
        let cache = DeltaCache::default();
        let conv = ConversationId::generate();
        let agent = AgentId::new("forge");
        let skills = vec![SkillListing::new("a", "A")];

        let _ = cache.delta(conv, agent.clone(), &skills).await;
        let actual = cache.delta(conv, agent, &skills).await;

        assert!(actual.is_empty());
    }

    #[tokio::test]
    async fn test_delta_cache_new_skill_returned() {
        let cache = DeltaCache::default();
        let conv = ConversationId::generate();
        let agent = AgentId::new("forge");

        let first = vec![SkillListing::new("a", "A")];
        let _ = cache.delta(conv, agent.clone(), &first).await;

        let second = vec![SkillListing::new("a", "A"), SkillListing::new("b", "B")];
        let actual = cache.delta(conv, agent, &second).await;

        let expected = vec![SkillListing::new("b", "B")];
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_delta_cache_independent_per_agent() {
        let cache = DeltaCache::default();
        let conv = ConversationId::generate();
        let skills = vec![SkillListing::new("a", "A")];

        let _ = cache.delta(conv, AgentId::new("forge"), &skills).await;
        let actual = cache.delta(conv, AgentId::new("sage"), &skills).await;

        // sage has never seen the skill, so it gets the full list back.
        assert_eq!(actual, skills);
    }

    #[tokio::test]
    async fn test_delta_cache_independent_per_conversation() {
        let cache = DeltaCache::default();
        let agent = AgentId::new("forge");
        let skills = vec![SkillListing::new("a", "A")];

        let _ = cache
            .delta(ConversationId::generate(), agent.clone(), &skills)
            .await;
        let actual = cache
            .delta(ConversationId::generate(), agent, &skills)
            .await;

        assert_eq!(actual, skills);
    }

    // --- Handler integration ---------------------------------------------

    /// Minimal mock service that returns a fixed skill list and counts
    /// invocations.
    struct MockSkillService {
        skills: Vec<Skill>,
        calls: AtomicUsize,
        invalidations: AtomicUsize,
    }

    impl MockSkillService {
        fn new(skills: Vec<Skill>) -> Self {
            Self {
                skills,
                calls: AtomicUsize::new(0),
                invalidations: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl SkillFetchService for MockSkillService {
        async fn fetch_skill(&self, name: String) -> anyhow::Result<Skill> {
            self.skills
                .iter()
                .find(|s| s.name == name)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("not found"))
        }

        async fn list_skills(&self) -> anyhow::Result<Vec<Skill>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.skills.clone())
        }

        async fn invalidate_cache(&self) {
            self.invalidations.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn fixture_conversation() -> Conversation {
        let mut conv = Conversation::generate();
        conv.context = Some(Context::default());
        conv
    }

    fn fixture_agent(agent_id: &str) -> Agent {
        Agent::new(
            AgentId::new(agent_id),
            ProviderId::FORGE,
            ModelId::new("test-model"),
        )
    }

    fn fixture_event(agent_id: &str) -> EventData<RequestPayload> {
        let agent = fixture_agent(agent_id);
        EventData::new(agent, ModelId::new("test-model"), RequestPayload::new(0))
    }

    #[tokio::test]
    async fn test_handler_injects_reminder_on_first_request() {
        let service = Arc::new(MockSkillService::new(vec![Skill::new(
            "pdf",
            "",
            "Handle PDF files",
        )]));
        let handler = SkillListingHandler::new(service);
        let mut conv = fixture_conversation();
        let event = fixture_event("forge");

        handler.handle(&event, &mut conv).await.unwrap();

        let ctx = conv.context.as_ref().unwrap();
        assert_eq!(ctx.messages.len(), 1);
        let msg = &ctx.messages[0];
        let content = msg.content().unwrap();
        assert!(
            content.contains("<system_reminder>"),
            "expected reminder envelope, got: {content}"
        );
        assert!(
            content.contains("pdf"),
            "expected skill name in catalog, got: {content}"
        );
    }

    #[tokio::test]
    async fn test_handler_skips_on_second_request_if_unchanged() {
        let service = Arc::new(MockSkillService::new(vec![Skill::new(
            "pdf",
            "",
            "Handle PDF files",
        )]));
        let handler = SkillListingHandler::new(service);
        let mut conv = fixture_conversation();
        let event = fixture_event("forge");

        handler.handle(&event, &mut conv).await.unwrap();
        handler.handle(&event, &mut conv).await.unwrap();

        let ctx = conv.context.as_ref().unwrap();
        // Only one reminder should have been injected.
        assert_eq!(ctx.messages.len(), 1);
    }

    #[tokio::test]
    async fn test_handler_noop_on_empty_skill_list() {
        let service = Arc::new(MockSkillService::new(vec![]));
        let handler = SkillListingHandler::new(service);
        let mut conv = fixture_conversation();
        let event = fixture_event("forge");

        handler.handle(&event, &mut conv).await.unwrap();

        let ctx = conv.context.as_ref().unwrap();
        assert!(ctx.messages.is_empty());
    }

    #[tokio::test]
    async fn test_handler_noop_when_context_missing() {
        let service = Arc::new(MockSkillService::new(vec![Skill::new(
            "pdf",
            "",
            "Handle PDF files",
        )]));
        let handler = SkillListingHandler::new(service);
        let mut conv = Conversation::generate();
        // Deliberately leave `conv.context = None`.
        let event = fixture_event("forge");

        let result = handler.handle(&event, &mut conv).await;

        assert!(result.is_ok());
        assert!(conv.context.is_none());
    }

    #[tokio::test]
    async fn test_handler_independent_per_agent() {
        let service = Arc::new(MockSkillService::new(vec![Skill::new(
            "pdf",
            "",
            "Handle PDF files",
        )]));
        let handler = SkillListingHandler::new(service);
        let mut conv = fixture_conversation();

        handler
            .handle(&fixture_event("forge"), &mut conv)
            .await
            .unwrap();
        handler
            .handle(&fixture_event("sage"), &mut conv)
            .await
            .unwrap();

        let ctx = conv.context.as_ref().unwrap();
        // Each agent should have received its own reminder.
        assert_eq!(ctx.messages.len(), 2);
    }

    #[test]
    fn test_budget_tokens_default() {
        let service = Arc::new(MockSkillService::new(vec![]));
        let handler = SkillListingHandler::new(service);
        // 200k * 0.01 = 2000
        assert_eq!(handler.budget_tokens(), 2_000);
    }

    #[test]
    fn test_budget_tokens_custom() {
        let service = Arc::new(MockSkillService::new(vec![]));
        let handler = SkillListingHandler::new(service)
            .context_tokens(10_000)
            .budget_fraction(0.05);
        // 10k * 0.05 = 500
        assert_eq!(handler.budget_tokens(), 500);
    }

    // --- Path matcher -----------------------------------------------------

    #[test]
    fn test_is_skill_file_path_plugin_skill() {
        assert!(is_skill_file_path(
            "/Users/me/forge/plugins/office/skills/pdf/SKILL.md"
        ));
    }

    #[test]
    fn test_is_skill_file_path_builtin_skill() {
        assert!(is_skill_file_path(
            "crates/forge_repo/src/skills/create-skill/SKILL.md"
        ));
    }

    #[test]
    fn test_is_skill_file_path_user_skill() {
        assert!(is_skill_file_path("~/forge/skills/my-tool/SKILL.md"));
    }

    #[test]
    fn test_is_skill_file_path_windows_separator() {
        assert!(is_skill_file_path(
            r"C:\Users\me\forge\skills\my-tool\SKILL.md"
        ));
    }

    #[test]
    fn test_is_skill_file_path_rejects_regular_markdown() {
        assert!(!is_skill_file_path(
            "crates/forge_repo/src/skills/README.md"
        ));
    }

    #[test]
    fn test_is_skill_file_path_rejects_no_skills_dir() {
        assert!(!is_skill_file_path("docs/SKILL.md"));
    }

    #[test]
    fn test_is_skill_file_path_rejects_case_variant() {
        assert!(!is_skill_file_path("forge/skills/my-tool/skill.md"));
    }

    // --- FS mutation tool matcher -----------------------------------------

    #[test]
    fn test_is_fs_mutation_tool_known() {
        assert!(is_fs_mutation_tool("write"));
        assert!(is_fs_mutation_tool("patch"));
        assert!(is_fs_mutation_tool("multi_patch"));
        assert!(is_fs_mutation_tool("remove"));
    }

    #[test]
    fn test_is_fs_mutation_tool_unknown() {
        assert!(!is_fs_mutation_tool("read"));
        assert!(!is_fs_mutation_tool("shell"));
        assert!(!is_fs_mutation_tool("skill_fetch"));
    }

    // --- Tool call path extraction ----------------------------------------

    #[test]
    fn test_extract_path_from_write() {
        use forge_domain::{ToolCallArguments, ToolCallFull, ToolName};
        let call = ToolCallFull {
            name: ToolName::new("write"),
            call_id: None,
            arguments: ToolCallArguments::from_json(
                r##"{"file_path": "/tmp/skills/foo/SKILL.md", "content": "# foo"}"##,
            ),
            thought_signature: None,
        };
        assert_eq!(
            extract_fs_target_path(&call),
            Some("/tmp/skills/foo/SKILL.md".to_string())
        );
    }

    #[test]
    fn test_extract_path_from_remove() {
        use forge_domain::{ToolCallArguments, ToolCallFull, ToolName};
        let call = ToolCallFull {
            name: ToolName::new("remove"),
            call_id: None,
            arguments: ToolCallArguments::from_json(r#"{"path": "/tmp/skills/foo/SKILL.md"}"#),
            thought_signature: None,
        };
        assert_eq!(
            extract_fs_target_path(&call),
            Some("/tmp/skills/foo/SKILL.md".to_string())
        );
    }

    #[test]
    fn test_extract_path_missing_field() {
        use forge_domain::{ToolCallArguments, ToolCallFull, ToolName};
        let call = ToolCallFull {
            name: ToolName::new("write"),
            call_id: None,
            arguments: ToolCallArguments::from_json(r#"{"content": "hello"}"#),
            thought_signature: None,
        };
        assert_eq!(extract_fs_target_path(&call), None);
    }

    // --- SkillCacheInvalidator end-to-end ---------------------------------

    #[derive(Default)]
    struct InvalidationCountingService {
        invalidate_calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl SkillFetchService for InvalidationCountingService {
        async fn list_skills(&self) -> anyhow::Result<Vec<Skill>> {
            Ok(vec![])
        }

        async fn fetch_skill(&self, _name: String) -> anyhow::Result<Skill> {
            Err(anyhow::anyhow!("not implemented"))
        }

        async fn invalidate_cache(&self) {
            self.invalidate_calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn fixture_toolcall_end_event(
        agent_id: &str,
        tool_name: &str,
        args_json: &str,
    ) -> EventData<forge_domain::ToolcallEndPayload> {
        use forge_domain::{
            ToolCallArguments, ToolCallFull, ToolName, ToolResult, ToolcallEndPayload,
        };
        let agent = Agent::new(
            AgentId::new(agent_id),
            ProviderId::FORGE,
            ModelId::new("test-model"),
        );
        let tool_call = ToolCallFull {
            name: ToolName::new(tool_name),
            call_id: None,
            arguments: ToolCallArguments::from_json(args_json),
            thought_signature: None,
        };
        let result = ToolResult::new(ToolName::new(tool_name));
        let payload = ToolcallEndPayload::new(tool_call, result);
        EventData::new(agent, ModelId::new("test-model"), payload)
    }

    #[tokio::test]
    async fn test_invalidator_fires_on_skill_write() {
        let service = Arc::new(InvalidationCountingService::default());
        let handler = SkillCacheInvalidator::new(service.clone());
        let mut conv = fixture_conversation();

        let event = fixture_toolcall_end_event(
            "forge",
            "write",
            r##"{"file_path": "/forge/skills/new/SKILL.md", "content": "# new"}"##,
        );
        handler.handle(&event, &mut conv).await.unwrap();

        assert_eq!(service.invalidate_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_invalidator_skips_non_skill_write() {
        let service = Arc::new(InvalidationCountingService::default());
        let handler = SkillCacheInvalidator::new(service.clone());
        let mut conv = fixture_conversation();

        let event = fixture_toolcall_end_event(
            "forge",
            "write",
            r#"{"file_path": "/tmp/unrelated.txt", "content": "hello"}"#,
        );
        handler.handle(&event, &mut conv).await.unwrap();

        assert_eq!(service.invalidate_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_invalidator_skips_non_mutation_tool() {
        let service = Arc::new(InvalidationCountingService::default());
        let handler = SkillCacheInvalidator::new(service.clone());
        let mut conv = fixture_conversation();

        let event = fixture_toolcall_end_event(
            "forge",
            "read",
            r#"{"file_path": "/forge/skills/new/SKILL.md"}"#,
        );
        handler.handle(&event, &mut conv).await.unwrap();

        assert_eq!(service.invalidate_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_invalidator_fires_on_skill_remove() {
        let service = Arc::new(InvalidationCountingService::default());
        let handler = SkillCacheInvalidator::new(service.clone());
        let mut conv = fixture_conversation();

        let event = fixture_toolcall_end_event(
            "forge",
            "remove",
            r#"{"path": "/forge/skills/old/SKILL.md"}"#,
        );
        handler.handle(&event, &mut conv).await.unwrap();

        assert_eq!(service.invalidate_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_invalidator_fires_on_skill_patch() {
        let service = Arc::new(InvalidationCountingService::default());
        let handler = SkillCacheInvalidator::new(service.clone());
        let mut conv = fixture_conversation();

        let event = fixture_toolcall_end_event(
            "forge",
            "patch",
            r##"{"file_path": "/forge/skills/existing/SKILL.md", "old_string": "a", "new_string": "b"}"##,
        );
        handler.handle(&event, &mut conv).await.unwrap();

        assert_eq!(service.invalidate_calls.load(Ordering::SeqCst), 1);
    }
}
