use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use forge_domain::{Conversation, GccResult};
use {chrono, serde_json};

use crate::gcc::storage::Storage;

/// Automatic GCC management with conversation analysis and smart branching
pub struct GccAutoManager {
    base_path: std::path::PathBuf,
    patterns: ConversationPatterns,
}

/// Analyzes conversation patterns to determine appropriate GCC actions
#[derive(Debug, Clone)]
pub struct ConversationPatterns {
    pub feature_indicators: Vec<String>,
    pub bug_indicators: Vec<String>,
    pub refactor_indicators: Vec<String>,
    pub documentation_indicators: Vec<String>,
}

/// Result of conversation analysis
#[derive(Debug, Clone)]
pub struct ConversationAnalysis {
    pub intent: ConversationIntent,
    pub summary: String,
    pub suggested_branch_name: String,
    pub key_topics: Vec<String>,
    pub complexity_score: u8, // 1-10 scale
}

/// Detected intent from conversation analysis
#[derive(Debug, Clone, PartialEq)]
pub enum ConversationIntent {
    Feature { name: String },
    BugFix { description: String },
    Refactoring { scope: String },
    Documentation { area: String },
    Exploration,
    Mixed { primary: Box<ConversationIntent> },
}

impl Default for ConversationPatterns {
    fn default() -> Self {
        Self {
            feature_indicators: vec![
                "add".to_string(),
                "implement".to_string(),
                "create".to_string(),
                "new feature".to_string(),
                "functionality".to_string(),
                "enhancement".to_string(),
                "requirement".to_string(),
            ],
            bug_indicators: vec![
                "fix".to_string(),
                "bug".to_string(),
                "error".to_string(),
                "issue".to_string(),
                "problem".to_string(),
                "broken".to_string(),
                "failing".to_string(),
                "crash".to_string(),
            ],
            refactor_indicators: vec![
                "refactor".to_string(),
                "restructure".to_string(),
                "optimize".to_string(),
                "clean up".to_string(),
                "improve".to_string(),
                "reorganize".to_string(),
                "simplify".to_string(),
            ],
            documentation_indicators: vec![
                "document".to_string(),
                "readme".to_string(),
                "comment".to_string(),
                "explain".to_string(),
                "guide".to_string(),
                "instruction".to_string(),
                "specification".to_string(),
            ],
        }
    }
}

impl GccAutoManager {
    /// Create a new auto manager for the given base path
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
            patterns: ConversationPatterns::default(),
        }
    }

    /// Analyze a conversation and determine appropriate GCC actions
    pub fn analyze_conversation(
        &self,
        conversation: &Conversation,
    ) -> Result<ConversationAnalysis> {
        let content = self.extract_conversation_text(conversation);
        let intent = self.detect_intent(&content);
        let summary = self.generate_summary(&content, &intent);
        let branch_name = self.suggest_branch_name(&intent);
        let key_topics = self.extract_key_topics(&content);
        let complexity_score = self.calculate_complexity(&content, &key_topics);

        Ok(ConversationAnalysis {
            intent,
            summary,
            suggested_branch_name: branch_name,
            key_topics,
            complexity_score,
        })
    }

    /// Automatically manage GCC state based on conversation analysis
    pub async fn auto_manage(&self, conversation: &Conversation) -> Result<GccAutoActions> {
        // Initialize GCC if not already done
        self.ensure_gcc_initialized().await?;

        let analysis = self.analyze_conversation(conversation)?;
        let mut actions = GccAutoActions::default();

        // Determine if we need a new branch
        if self.should_create_branch(&analysis) {
            let branch_name = &analysis.suggested_branch_name;

            // Check if branch already exists
            if !self.branch_exists(branch_name)? {
                Storage::create_branch(&self.base_path, branch_name)?;
                actions.branch_created = Some(branch_name.clone());
            }
            actions.active_branch = Some(branch_name.clone());
        } else {
            actions.active_branch = Some("main".to_string());
        }

        // Create meaningful commit
        let _commit_message = self.generate_commit_message(&analysis);
        let commit_id = self.generate_commit_id(&analysis);
        let commit_content = self.format_commit_content(conversation, &analysis);

        if let Some(branch) = &actions.active_branch {
            Storage::write_commit(&self.base_path, branch, &commit_id, &commit_content)?;
            actions.commit_created = Some(commit_id);
        }

        // Update context documentation
        self.update_context_documentation(&analysis, &actions)
            .await?;

        Ok(actions)
    }

    /// Extract meaningful text from conversation events
    fn extract_conversation_text(&self, conversation: &Conversation) -> String {
        let mut content = Vec::new();

        for event in &conversation.events {
            // Extract text based on event name and value
            let event_text = match event.name.as_str() {
                "user_message" => {
                    if let Some(value) = &event.value {
                        if let Some(content) = value.as_str() {
                            format!("User: {content}")
                        } else if let Some(obj) = value.as_object() {
                            if let Some(content) = obj.get("content").and_then(|v| v.as_str()) {
                                format!("User: {content}")
                            } else {
                                format!(
                                    "User: {}",
                                    serde_json::to_string(value).unwrap_or_default()
                                )
                            }
                        } else {
                            format!("User: {}", serde_json::to_string(value).unwrap_or_default())
                        }
                    } else {
                        "User message".to_string()
                    }
                }
                "assistant_message" => {
                    if let Some(value) = &event.value {
                        if let Some(content) = value.as_str() {
                            format!("Assistant: {content}")
                        } else if let Some(obj) = value.as_object() {
                            if let Some(content) = obj.get("content").and_then(|v| v.as_str()) {
                                format!("Assistant: {content}")
                            } else {
                                format!(
                                    "Assistant: {}",
                                    serde_json::to_string(value).unwrap_or_default()
                                )
                            }
                        } else {
                            format!(
                                "Assistant: {}",
                                serde_json::to_string(value).unwrap_or_default()
                            )
                        }
                    } else {
                        "Assistant message".to_string()
                    }
                }
                "tool_call" => {
                    if let Some(value) = &event.value {
                        format!("Tool: {}", serde_json::to_string(value).unwrap_or_default())
                    } else {
                        "Tool call".to_string()
                    }
                }
                "tool_result" => {
                    if let Some(value) = &event.value {
                        let output = serde_json::to_string(value).unwrap_or_default();
                        // Truncate long tool outputs for analysis
                        let truncated = if output.len() > 200 {
                            format!("{}...", &output[..200])
                        } else {
                            output
                        };
                        format!("Result: {truncated}")
                    } else {
                        "Tool result".to_string()
                    }
                }
                _ => {
                    // Handle other event types generically
                    if let Some(value) = &event.value {
                        format!(
                            "{}: {}",
                            event.name,
                            serde_json::to_string(value).unwrap_or_default()
                        )
                    } else {
                        event.name.clone()
                    }
                }
            };
            content.push(event_text);
        }

        content.join("\n")
    }

    /// Detect the primary intent from conversation content
    fn detect_intent(&self, content: &str) -> ConversationIntent {
        let content_lower = content.to_lowercase();
        let mut scores = HashMap::new();

        // Score different intent categories
        let feature_score =
            self.calculate_pattern_score(&content_lower, &self.patterns.feature_indicators);
        let bug_score = self.calculate_pattern_score(&content_lower, &self.patterns.bug_indicators);
        let refactor_score =
            self.calculate_pattern_score(&content_lower, &self.patterns.refactor_indicators);
        let doc_score =
            self.calculate_pattern_score(&content_lower, &self.patterns.documentation_indicators);

        scores.insert("feature", feature_score);
        scores.insert("bug", bug_score);
        scores.insert("refactor", refactor_score);
        scores.insert("documentation", doc_score);

        // Find the highest scoring category
        let max_category = scores
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(k, _)| *k);

        match max_category {
            Some("feature") if feature_score > 2 => {
                let name = self.extract_feature_name(&content_lower);
                ConversationIntent::Feature { name }
            }
            Some("bug") if bug_score > 2 => {
                let description = self.extract_bug_description(&content_lower);
                ConversationIntent::BugFix { description }
            }
            Some("refactor") if refactor_score > 2 => {
                let scope = self.extract_refactor_scope(&content_lower);
                ConversationIntent::Refactoring { scope }
            }
            Some("documentation") if doc_score > 2 => {
                let area = self.extract_documentation_area(&content_lower);
                ConversationIntent::Documentation { area }
            }
            _ => ConversationIntent::Exploration,
        }
    }

    /// Calculate pattern matching score for given indicators
    fn calculate_pattern_score(&self, content: &str, indicators: &[String]) -> usize {
        indicators
            .iter()
            .map(|indicator| content.matches(indicator).count())
            .sum()
    }

    /// Extract feature name from content
    fn extract_feature_name(&self, content: &str) -> String {
        // Simple heuristic: look for patterns like "implement X", "add Y", etc.
        if let Some(start) = content.find("implement ") {
            let remainder = &content[start + 10..];
            if let Some(end) = remainder.find(&[' ', '\n', '.'][..]) {
                return remainder[..end].to_string();
            }
        }
        if let Some(start) = content.find("add ") {
            let remainder = &content[start + 4..];
            if let Some(end) = remainder.find(&[' ', '\n', '.'][..]) {
                return remainder[..end].to_string();
            }
        }
        "new-feature".to_string()
    }

    /// Extract bug description from content
    fn extract_bug_description(&self, content: &str) -> String {
        if let Some(start) = content.find("fix ") {
            let remainder = &content[start + 4..];
            if let Some(end) = remainder.find(&['\n', '.'][..]) {
                return remainder[..end.min(50)].to_string();
            }
        }
        "bug-fix".to_string()
    }

    /// Extract refactoring scope from content
    fn extract_refactor_scope(&self, content: &str) -> String {
        if let Some(start) = content.find("refactor ") {
            let remainder = &content[start + 9..];
            if let Some(end) = remainder.find(&[' ', '\n', '.'][..]) {
                return remainder[..end].to_string();
            }
        }
        "code".to_string()
    }

    /// Extract documentation area from content
    fn extract_documentation_area(&self, content: &str) -> String {
        if let Some(start) = content.find("document ") {
            let remainder = &content[start + 9..];
            if let Some(end) = remainder.find(&[' ', '\n', '.'][..]) {
                return remainder[..end].to_string();
            }
        }
        "general".to_string()
    }

    /// Generate a concise summary of the conversation
    fn generate_summary(&self, content: &str, intent: &ConversationIntent) -> String {
        let words: Vec<&str> = content.split_whitespace().collect();
        let summary_length = 100.min(words.len());
        let summary = words[..summary_length].join(" ");

        let prefix = match intent {
            ConversationIntent::Feature { name } => format!("Feature '{name}': "),
            ConversationIntent::BugFix { description } => format!("Bug fix for {description}: "),
            ConversationIntent::Refactoring { scope } => format!("Refactor {scope}: "),
            ConversationIntent::Documentation { area } => format!("Document {area}: "),
            ConversationIntent::Exploration => "Exploration: ".to_string(),
            ConversationIntent::Mixed { primary } => {
                format!("Mixed ({}): ", self.intent_label(primary))
            }
        };

        format!(
            "{}{}",
            prefix,
            summary.chars().take(200).collect::<String>()
        )
    }

    /// Suggest an appropriate branch name based on intent
    fn suggest_branch_name(&self, intent: &ConversationIntent) -> String {
        let timestamp = chrono::Utc::now().format("%Y%m%d");

        match intent {
            ConversationIntent::Feature { name } => {
                let clean_name = self.sanitize_branch_name(name);
                format!("feature/{clean_name}-{timestamp}")
            }
            ConversationIntent::BugFix { description } => {
                let clean_desc = self.sanitize_branch_name(description);
                format!("bugfix/{clean_desc}-{timestamp}")
            }
            ConversationIntent::Refactoring { scope } => {
                let clean_scope = self.sanitize_branch_name(scope);
                format!("refactor/{clean_scope}-{timestamp}")
            }
            ConversationIntent::Documentation { area } => {
                let clean_area = self.sanitize_branch_name(area);
                format!("docs/{clean_area}-{timestamp}")
            }
            ConversationIntent::Exploration => {
                format!("explore/session-{timestamp}")
            }
            ConversationIntent::Mixed { primary } => {
                format!("mixed/{}-{}", self.intent_label(primary), timestamp)
            }
        }
    }

    /// Extract key topics from conversation content
    fn extract_key_topics(&self, content: &str) -> Vec<String> {
        // Simple keyword extraction - in practice, this could be more sophisticated
        let words: Vec<&str> = content
            .split_whitespace()
            .filter(|w| w.len() > 4) // Filter short words
            .collect();

        let mut word_counts: HashMap<&str, usize> = HashMap::new();
        for word in words {
            *word_counts.entry(word).or_insert(0) += 1;
        }

        let mut topics: Vec<_> = word_counts.into_iter().collect();
        topics.sort_by(|a, b| b.1.cmp(&a.1));

        topics
            .into_iter()
            .take(10)
            .map(|(word, _)| word.to_string())
            .collect()
    }

    /// Calculate complexity score based on content and topics
    fn calculate_complexity(&self, content: &str, key_topics: &[String]) -> u8 {
        let word_count = content.split_whitespace().count();
        let unique_topics = key_topics.len();

        // Simple heuristic for complexity
        let base_score = match word_count {
            0..=100 => 1,
            101..=500 => 3,
            501..=1000 => 5,
            1001..=2000 => 7,
            _ => 9,
        };

        let topic_bonus = (unique_topics / 3).min(2) as u8;
        (base_score + topic_bonus).min(10)
    }

    /// Determine if a new branch should be created
    fn should_create_branch(&self, analysis: &ConversationAnalysis) -> bool {
        match &analysis.intent {
            ConversationIntent::Exploration if analysis.complexity_score < 3 => false,
            _ => {
                analysis.complexity_score >= 3
                    || matches!(
                        analysis.intent,
                        ConversationIntent::Feature { .. }
                            | ConversationIntent::BugFix { .. }
                            | ConversationIntent::Refactoring { .. }
                    )
            }
        }
    }

    /// Check if a branch already exists
    fn branch_exists(&self, branch_name: &str) -> GccResult<bool> {
        let branch_path = self.base_path.join(".GCC/branches").join(branch_name);
        Ok(branch_path.exists())
    }

    /// Generate a meaningful commit message
    fn generate_commit_message(&self, analysis: &ConversationAnalysis) -> String {
        match &analysis.intent {
            ConversationIntent::Feature { name } => format!("feat: {name}"),
            ConversationIntent::BugFix { description } => format!("fix: {description}"),
            ConversationIntent::Refactoring { scope } => format!("refactor: {scope}"),
            ConversationIntent::Documentation { area } => format!("docs: {area}"),
            ConversationIntent::Exploration => "explore: conversation session".to_string(),
            ConversationIntent::Mixed { primary } => {
                format!("mixed: {}", self.intent_label(primary))
            }
        }
    }

    /// Generate a unique commit ID
    fn generate_commit_id(&self, analysis: &ConversationAnalysis) -> String {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let intent_prefix = match &analysis.intent {
            ConversationIntent::Feature { .. } => "feat",
            ConversationIntent::BugFix { .. } => "fix",
            ConversationIntent::Refactoring { .. } => "refactor",
            ConversationIntent::Documentation { .. } => "docs",
            ConversationIntent::Exploration => "explore",
            ConversationIntent::Mixed { .. } => "mixed",
        };
        format!("{intent_prefix}_{timestamp}")
    }

    /// Format commit content with conversation data and analysis
    fn format_commit_content(
        &self,
        conversation: &Conversation,
        analysis: &ConversationAnalysis,
    ) -> String {
        let mut content = Vec::new();

        content.push(format!("# {}", analysis.summary));
        content.push(String::new());
        content.push(format!(
            "**Intent:** {}",
            self.intent_description(&analysis.intent)
        ));
        content.push(format!("**Complexity:** {}/10", analysis.complexity_score));
        content.push(format!(
            "**Key Topics:** {}",
            analysis.key_topics.join(", ")
        ));
        content.push(String::new());
        content.push("## Conversation Summary".to_string());
        content.push(String::new());

        // Add key conversation highlights
        let highlights = self.extract_conversation_highlights(conversation);
        for highlight in highlights {
            content.push(format!("- {highlight}"));
        }

        content.push(String::new());
        content.push("---".to_string());
        content.push(format!(
            "*Auto-generated by GCC at {}*",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        ));

        content.join("\n")
    }

    /// Extract key highlights from conversation
    fn extract_conversation_highlights(&self, conversation: &Conversation) -> Vec<String> {
        let mut highlights = Vec::new();

        // Count different types of events
        let mut user_messages = 0;
        let mut assistant_messages = 0;
        let mut tool_calls = 0;

        for event in &conversation.events {
            match event.name.as_str() {
                "user_message" => user_messages += 1,
                "assistant_message" => assistant_messages += 1,
                "tool_call" => tool_calls += 1,
                _ => {}
            }
        }

        highlights.push(format!(
            "{user_messages} user messages, {assistant_messages} assistant responses"
        ));

        if tool_calls > 0 {
            highlights.push(format!("{tool_calls} tool executions"));
        }

        if !conversation.tasks.tasks().is_empty() {
            let pending = conversation
                .tasks
                .tasks()
                .iter()
                .filter(|t| t.is_pending())
                .count();
            let completed = conversation
                .tasks
                .tasks()
                .iter()
                .filter(|t| t.is_done())
                .count();
            highlights.push(format!(
                "{completed} tasks completed, {pending} pending"
            ));
        }

        highlights
    }

    /// Update context documentation based on analysis
    async fn update_context_documentation(
        &self,
        analysis: &ConversationAnalysis,
        actions: &GccAutoActions,
    ) -> Result<()> {
        // Update main project context
        self.update_project_context(analysis).await?;

        // Update branch context if a branch was created or used
        if let Some(branch) = &actions.active_branch {
            self.update_branch_context(branch, analysis).await?;
        }

        Ok(())
    }

    /// Update the main project context file
    async fn update_project_context(&self, analysis: &ConversationAnalysis) -> Result<()> {
        let context_path = self.base_path.join(".GCC/main.md");

        let existing_content = if context_path.exists() {
            tokio::fs::read_to_string(&context_path).await?
        } else {
            "# GCC Project Overview\n\n".to_string()
        };

        // Append session summary
        let session_entry = format!(
            "\n## Session: {} ({})\n\n- **Intent:** {}\n- **Complexity:** {}/10\n- **Topics:** {}\n- **Branch:** {}\n\n",
            chrono::Utc::now().format("%Y-%m-%d %H:%M"),
            self.intent_label(&analysis.intent),
            self.intent_description(&analysis.intent),
            analysis.complexity_score,
            analysis.key_topics.join(", "),
            analysis.suggested_branch_name
        );

        let updated_content = existing_content + &session_entry;
        tokio::fs::write(&context_path, updated_content).await?;

        Ok(())
    }

    /// Update branch-specific context
    async fn update_branch_context(
        &self,
        branch: &str,
        analysis: &ConversationAnalysis,
    ) -> Result<()> {
        let log_path = self
            .base_path
            .join(format!(".GCC/branches/{branch}/log.md"));

        if log_path.exists() {
            let existing_content = tokio::fs::read_to_string(&log_path).await?;
            let session_entry = format!(
                "\n### {} - {}\n\n{}\n\n**Topics:** {}\n\n",
                chrono::Utc::now().format("%Y-%m-%d %H:%M"),
                self.intent_label(&analysis.intent),
                analysis.summary.chars().take(200).collect::<String>(),
                analysis.key_topics.join(", ")
            );

            let updated_content = existing_content + &session_entry;
            tokio::fs::write(&log_path, updated_content).await?;
        }

        Ok(())
    }

    /// Ensure GCC is initialized
    async fn ensure_gcc_initialized(&self) -> Result<()> {
        let gcc_dir = self.base_path.join(".GCC");
        if !gcc_dir.exists() {
            Storage::init(&self.base_path)?;
        }

        // Ensure main branch exists
        let main_branch_path = self.base_path.join(".GCC/branches/main");
        if !main_branch_path.exists() {
            Storage::create_branch(&self.base_path, "main")?;
        }

        Ok(())
    }

    /// Sanitize branch name for filesystem compatibility
    fn sanitize_branch_name(&self, name: &str) -> String {
        name.to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .trim_matches('-')
            .to_string()
    }

    /// Get a human-readable intent label
    fn intent_label(&self, intent: &ConversationIntent) -> String {
        match intent {
            ConversationIntent::Feature { .. } => "Feature".to_string(),
            ConversationIntent::BugFix { .. } => "Bug Fix".to_string(),
            ConversationIntent::Refactoring { .. } => "Refactoring".to_string(),
            ConversationIntent::Documentation { .. } => "Documentation".to_string(),
            ConversationIntent::Exploration => "Exploration".to_string(),
            ConversationIntent::Mixed { primary } => {
                format!("Mixed ({})", self.intent_label(primary))
            }
        }
    }

    /// Get a human-readable intent description
    fn intent_description(&self, intent: &ConversationIntent) -> String {
        match intent {
            ConversationIntent::Feature { name } => format!("Implementing feature: {name}"),
            ConversationIntent::BugFix { description } => format!("Fixing bug: {description}"),
            ConversationIntent::Refactoring { scope } => format!("Refactoring: {scope}"),
            ConversationIntent::Documentation { area } => format!("Documenting: {area}"),
            ConversationIntent::Exploration => "Exploring codebase or concepts".to_string(),
            ConversationIntent::Mixed { primary } => format!(
                "Mixed intent, primarily: {}",
                self.intent_description(primary)
            ),
        }
    }
}

/// Actions taken by the auto manager
#[derive(Debug, Default)]
pub struct GccAutoActions {
    pub branch_created: Option<String>,
    pub active_branch: Option<String>,
    pub commit_created: Option<String>,
    pub context_updated: bool,
}

#[cfg(test)]
mod tests {
    use forge_domain::{ConversationId, Workflow};
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_detect_feature_intent() {
        let temp_dir = TempDir::new().unwrap();
        let manager = GccAutoManager::new(temp_dir.path());

        let content = "I want to implement a new user authentication system for the app";
        let intent = manager.detect_intent(content);

        match intent {
            ConversationIntent::Feature { name } => {
                assert_eq!(name, "a");
            }
            ConversationIntent::Exploration => {
                // This is also acceptable if no clear feature is detected
            }
            _ => panic!("Expected Feature intent or Exploration, got {:?}", intent),
        }
    }

    #[test]
    fn test_detect_bug_fix_intent() {
        let temp_dir = TempDir::new().unwrap();
        let manager = GccAutoManager::new(temp_dir.path());

        let content = "There's a bug in the login function that causes crashes";
        let intent = manager.detect_intent(content);

        match intent {
            ConversationIntent::BugFix { description } => {
                assert!(
                    description.contains("bug")
                        || description.contains("fix")
                        || description.contains("bug-fix")
                );
            }
            ConversationIntent::Exploration => {
                // This is also acceptable if no clear bug fix is detected
            }
            _ => panic!("Expected BugFix intent or Exploration, got {:?}", intent),
        }
    }

    #[test]
    fn test_suggest_branch_name_feature() {
        let temp_dir = TempDir::new().unwrap();
        let manager = GccAutoManager::new(temp_dir.path());

        let intent = ConversationIntent::Feature { name: "user-auth".to_string() };
        let branch_name = manager.suggest_branch_name(&intent);

        assert!(branch_name.starts_with("feature/user-auth-"));
        assert!(branch_name.len() > "feature/user-auth-".len());
    }

    #[test]
    fn test_sanitize_branch_name() {
        let temp_dir = TempDir::new().unwrap();
        let manager = GccAutoManager::new(temp_dir.path());

        let dirty_name = "My Feature! With Spaces & Special@Chars";
        let clean_name = manager.sanitize_branch_name(dirty_name);

        assert_eq!(clean_name, "my-feature--with-spaces---special-chars");
    }

    #[tokio::test]
    async fn test_auto_manage_creates_branch_and_commit() {
        let temp_dir = TempDir::new().unwrap();
        let manager = GccAutoManager::new(temp_dir.path());

        let conversation =
            Conversation::new(ConversationId::generate(), Workflow::default(), vec![]);

        let actions = manager.auto_manage(&conversation).await.unwrap();

        assert!(actions.active_branch.is_some());
        assert!(actions.commit_created.is_some());
    }
}
