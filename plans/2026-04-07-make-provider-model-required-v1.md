# Make `provider_id` and `model_id` Required Throughout the Stack

## Objective

Remove the `Option` wrappers from `provider_id` and `model_id` in `ModelConfig` (`forge_config`) and propagate this guarantee upward through domain types, the infra layer, service layer, API trait, and CLI — ensuring these two fields are always present together whenever a config object exists. The outer `Option<ModelConfig>` in `ForgeConfig` (representing "not yet configured") is preserved; only the inner fields become required.

**Key invariant being enforced:** A `ModelConfig`, `SessionConfig`, or `CommitConfig` instance is only valid when it carries both a provider ID and a model ID. Partial state (provider set, model absent) is no longer representable.

---

## Implementation Plan

### Phase 1 — Core Config Types (`forge_config`)

- [x] Task 1. **`crates/forge_config/src/model.rs` — `ModelConfig` fields**
  - Change `provider_id: Option<String>` → `provider_id: String`
  - Change `model_id: Option<String>` → `model_id: String`
  - Remove `Default` from the `#[derive(...)]` list (no meaningful default for required strings)
  - Change `#[setters(strip_option, into)]` → `#[setters(into)]` (strip_option is Option-specific)
  - Add an explicit `pub fn new(provider_id: impl Into<String>, model_id: impl Into<String>) -> Self` constructor for ergonomic construction

- [x] Task 2. **`crates/forge_config/src/legacy.rs` — `LegacyConfig::into_forge_config`**
  - Session construction: Use `and_then` so that a `ModelConfig` is only created when both the provider is set AND the model for that provider is found in the map. If the model lookup fails (provider in config but not in model map), silently produce `None` for `session`.
  - Commit construction: Replace the direct `Option<String>` field struct literal with `zip()` on `c.provider` and `c.model` — only create `ModelConfig::new(pid, mid)` when both are `Some`.
  - Suggest construction: Same `zip()` pattern as commit.

- [x] Task 3. **`crates/forge_config/src/reader.rs` — test fixtures**
  - Update the test at line ~200 that constructs `ModelConfig { provider_id: Some("anthropic"), model_id: Some("claude-3") }` to use `ModelConfig::new("anthropic", "claude-3")`.

---

### Phase 2 — Domain Types (`forge_domain`)

- [x] Task 4. **`crates/forge_domain/src/env.rs` — `SessionConfig`**
  - Change `provider_id: Option<String>` → `provider_id: String`
  - Change `model_id: Option<String>` → `model_id: String`
  - Remove `Default` from derives
  - Change `#[setters(strip_option, into)]` → `#[setters(into)]`
  - Add `pub fn new(provider_id: impl Into<String>, model_id: impl Into<String>) -> Self` constructor

- [x] Task 5. **`crates/forge_domain/src/env.rs` — `ConfigOperation` enum**
  - Remove the `SetProvider(ProviderId)` variant entirely. This variant was the only way to create the partial (provider-only, no model) state. With required fields, there is no valid partial operation — the single `SetModel(ProviderId, ModelId)` variant becomes the sole session-mutation operation.

- [x] Task 6. **`crates/forge_domain/src/commit_config.rs` — `CommitConfig`**
  - Change `provider: Option<ProviderId>` → `provider: ProviderId`
  - Change `model: Option<ModelId>` → `model: ModelId`
  - Remove `Default` from derives
  - Remove `#[serde(default, skip_serializing_if = "Option::is_none")]` from both fields (no longer optional)
  - Remove `#[merge(strategy = crate::merge::option)]` from both fields and consider removing the `Merge` derive entirely since the Option-specific merge strategy no longer applies
  - Change `#[setters(strip_option, into)]` → `#[setters(into)]`
  - Add `pub fn new(provider: impl Into<ProviderId>, model: impl Into<ModelId>) -> Self` constructor

---

### Phase 3 — Infrastructure Layer (`forge_infra`)

- [x] Task 7. **`crates/forge_infra/src/env.rs` — `apply_config_op` function**
  - Remove the `ConfigOperation::SetProvider` arm from the match (the variant is gone)
  - Simplify the `ConfigOperation::SetModel` arm: always create a fresh `ModelConfig::new(pid_str, mid_str)` — eliminate the conditional branch that checked whether the existing provider matched. The always-overwrite semantics are simpler and correct.
  - Simplify the `ConfigOperation::SetCommitConfig` arm: `CommitConfig` now carries required fields, so replace the `zip()` pattern with a direct `Some(ModelConfig::new(commit.provider.as_ref(), commit.model.as_str()))`.
  - `ConfigOperation::SetSuggestConfig` arm needs no logic change (it already creates a complete `ModelConfig`), but update the field access to use non-optional syntax.

- [x] Task 8. **`crates/forge_infra/src/env.rs` — tests**
  - Remove the `test_apply_config_op_set_provider` test (the operation no longer exists); replace with a test that verifies `SetModel` alone creates a complete session pair.
  - Update `test_apply_config_op_set_model_matching_provider`: The fixture currently sets `session = Some(ModelConfig { provider_id: Some("anthropic"), model_id: None })` — this state is no longer representable. Change the fixture to use a complete starting pair (e.g., `ModelConfig::new("anthropic", "old-model")`) and verify the model is replaced.
  - Update `test_apply_config_op_set_model_different_provider_replaces_session` to use `ModelConfig::new(...)` construction.

---

### Phase 4 — Service Layer (`forge_services`)

- [x] Task 9. **`crates/forge_services/src/app_config.rs` — `get_default_provider`**
  - Simplify: `session.provider_id` is now a `String`. Remove the double `and_then`/`as_ref` chain; use a single `.map(|s| ProviderId::from(s.provider_id.clone()))`.

- [x] Task 10. **`crates/forge_services/src/app_config.rs` — `get_provider_model`**
  - Simplify: `session.provider_id` and `session.model_id` are no longer `Option`. Remove all inner `.as_ref()` / `.map(...)` option-unwrapping on these fields. The provider comparison becomes a direct string equality check.

- [x] Task 11. **`crates/forge_services/src/app_config.rs` — remove `set_default_provider`**
  - Remove the entire `set_default_provider` method implementation. With required fields there is no valid write operation that sets only the provider without a model. All callers must use `set_default_provider_and_model` instead.

- [x] Task 12. **`crates/forge_services/src/app_config.rs` — `set_default_model`**
  - Simplify: Reading `session.provider_id` no longer requires an `and_then` chain — it is a plain `String`. Update the `provider_id` extraction and the inline cache update (`session.model_id = model.to_string()`).

- [x] Task 13. **`crates/forge_services/src/app_config.rs` — `get_commit_config`**
  - Simplify: `CommitConfig.provider` and `CommitConfig.model` are now required fields — remove the `.map(ProviderId::from)` and `.map(ModelId::new)` Option wrappers. Direct field construction is now `CommitConfig { provider: mc.provider_id.into(), model: ModelId::new(mc.model_id) }`.

- [x] Task 14. **`crates/forge_services/src/app_config.rs` — `get_suggest_config`**
  - Simplify: Replace the `zip()` trick with a direct construction. Since `ModelConfig` always has both fields present, reading `mc.provider_id` and `mc.model_id` is direct, and the `SuggestConfig` is always constructed (no more `and_then`).

- [x] Task 15. **`crates/forge_services/src/app_config.rs` — mock `update_environment` in tests**
  - Remove the `ConfigOperation::SetProvider` arm from the mock match
  - Simplify `ConfigOperation::SetModel` arm: always `config.session = Some(ModelConfig::new(pid_str, mid_str))`
  - Simplify `ConfigOperation::SetCommitConfig` arm: `CommitConfig` now has required fields, replace `zip()` with direct field access

- [x] Task 16. **`crates/forge_services/src/app_config.rs` — tests**
  - Update all test methods that call `set_default_provider()` to instead call `set_default_provider_and_model(provider_id, model_id)` with a valid model for that provider
  - Remove the `test_set_default_provider` test or replace it with a test for `set_default_provider_and_model`
  - Update `test_get_default_provider_when_configured_provider_not_available` to use `set_default_provider_and_model`

---

### Phase 5 — Application Layer (`forge_app`)

- [x] Task 17. **`crates/forge_app/src/services.rs` — `AppConfigService` trait**
  - Remove `set_default_provider()` from the trait declaration

- [x] Task 18. **`crates/forge_app/src/services.rs` — delegating `impl<I: Services> AppConfigService for I`**
  - Remove the `set_default_provider()` delegation method from this blanket impl

- [x] Task 19. **`crates/forge_app/src/command_generator.rs` — `MockServices` impl**
  - Remove `set_default_provider()` from the `AppConfigService` impl for `MockServices`

---

### Phase 6 — API Layer (`forge_api`)

- [x] Task 20. **`crates/forge_api/src/api.rs` — `API` trait**
  - Remove `set_default_provider()` from the trait declaration

- [x] Task 21. **`crates/forge_api/src/forge_api.rs` — `ForgeAPI` impl**
  - Remove the `set_default_provider()` method implementation from `ForgeAPI`

---

### Phase 7 — CLI / UI Layer (`forge_main`)

- [x] Task 22. **`crates/forge_main/src/ui.rs` — `activate_provider_with_model` (line ~2886)**
  - In the `else` branch (model is already compatible with the new provider), replace the call to `self.api.set_default_provider(provider.id.clone())` with `self.api.set_default_provider_and_model(provider.id.clone(), current_model_id)`. Restructure the surrounding code so that `current_model` (the `Option<ModelId>` captured on line ~2860) is accessible in this else branch — either by changing the match to not consume it, or by cloning it before the match.

- [x] Task 23. **`crates/forge_main/src/ui.rs` — `handle_config_get` (line ~3608–3624)**
  - In the `ConfigGetField::Commit` arm, simplify the `CommitConfig` field access: `provider` and `model` are no longer `Option`, so remove the `.map(...).unwrap_or_else(|| "Not set".to_string())` wrappers and use direct `.as_ref().to_string()` / `.as_str().to_string()`.

- [x] Task 24. **`crates/forge_main/src/ui.rs` — `handle_config_set` (line ~3547–3549)**
  - In the `ConfigSetField::Commit` arm, replace `forge_domain::CommitConfig::default().provider(...).model(...)` with the new `CommitConfig::new(provider, validated_model)` constructor (since `Default` is removed).

---

### Phase 8 — Snapshot Regeneration and Verification

- [x] Task 25. **Regenerate test snapshots**
  - Run `cargo insta test --accept` to regenerate any snapshot tests that capture `ModelConfig`, `CommitConfig`, or related output that changed due to the field type change.

- [x] Task 26. **Verify compilation and tests**
  - Run `cargo check` across the workspace to catch any remaining sites that still use `Option<String>` accessors on these fields.
  - Run `cargo insta test --accept` to verify all tests pass with the updated type contracts.

---

## Verification Criteria

- `ModelConfig.provider_id` and `ModelConfig.model_id` are `String`, not `Option<String>` — the compiler enforces their presence at every construction site
- `SessionConfig.provider_id` and `SessionConfig.model_id` are `String`, not `Option<String>`
- `CommitConfig.provider` and `CommitConfig.model` are `ProviderId` and `ModelId`, not wrapped in `Option`
- `ConfigOperation::SetProvider` no longer exists — all session mutations go through `SetModel(ProviderId, ModelId)`
- `set_default_provider()` is removed from `AppConfigService` trait, `API` trait, and all implementations
- The legacy config reader (`legacy.rs`) only creates a `ModelConfig` for session when both provider and the model for that provider are present in the legacy JSON
- `apply_config_op` in infra is simplified to always produce a complete `ModelConfig` from `SetModel`, with no conditional branch
- All tests pass: no test relies on partial config state (provider set, model absent)
- `get_commit_config()` still returns `Option<CommitConfig>` (outer `Option` remains — commit config may not be configured), but when `Some`, both `provider` and `model` are guaranteed

---

## Potential Risks and Mitigations

1. **Legacy config files with provider but no model**
   Mitigation: The `legacy.rs` change (Task 2) handles this silently by producing `session: None` instead of a partial `ModelConfig`. Users who had a provider set in the old `.config.json` but no corresponding model entry will find no default session — they will be prompted to configure provider+model together on first run.

2. **Test suite breadth — many tests build `ForgeConfig` with partial `ModelConfig`**
   Mitigation: The `Default` removal causes compile errors at every such site, making all affected tests immediately visible. Tasks 8 and 16 address the known test files; Task 25 catches snapshot regressions.

3. **`set_default_model()` still requires an existing session**
   Mitigation: `set_default_model()` is preserved but still requires a provider-set session (it reads `session.provider_id`). If called with no session, it returns `Error::NoDefaultProvider` — same behavior as before. This is correct and unchanged.

4. **`activate_provider_with_model` code restructure (Task 22) introduces subtle scope issue**
   Mitigation: The `current_model` variable must be made available in the else branch. The implementation can either `.clone()` the variable before the match or refactor the `needs_model_selection` computation to a helper that returns both the bool and the current model without consuming it.

5. **`CommitConfig` field access in `handle_config_get`**
   Mitigation: Since `get_commit_config()` still returns `Option<CommitConfig>`, the outer match on `Some(config)` is preserved. Only the inner field access changes from `.map(...).unwrap_or_else(...)` to direct access — a mechanical simplification.

---

## Alternative Approaches

1. **Keep `ModelConfig` with `Option` fields but add a validated wrapper type** — Introduce a `ValidatedModelConfig { provider_id: String, model_id: String }` and use it at the service/API boundary while keeping `ModelConfig` as a deserialization target. This avoids touching the legacy config path but adds an extra type conversion layer and duplicates the type hierarchy.

2. **Keep `set_default_provider()` as a transitional method that errors** — Rather than removing `set_default_provider()`, change its body to always return `Err("Use set_default_provider_and_model instead")`. This preserves the API shape at the cost of runtime errors instead of compile errors. Not preferred since compile-time guarantees are stronger.

3. **Make `CommitConfig` fields remain optional** — Only change `ModelConfig` and `SessionConfig`, leaving `CommitConfig.provider` and `CommitConfig.model` as `Option`. This avoids the simplification in `apply_config_op` for commit configs but is inconsistent with the stated goal of "APIs and other types are adjusted". The current `zip()` pattern in the infra already de-facto requires both, so making them required is the cleaner expression of the same invariant.
