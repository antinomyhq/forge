# Forge Architectural Refactoring Plan

## Objective

Transform the Forge codebase from its current architecture — characterized by god objects, massive delegation boilerplate, blurred layer boundaries, and inconsistent trait placement — into a clean, modular architecture with strict layer separation, minimal boilerplate, explicit dependency graphs, and clear ownership boundaries for every type and trait.

**Non-goals:** This plan does not change external behavior, public CLI interfaces, or the server protocol. Every step must preserve the full test suite (998 files, 165 snapshots).

---

## Phase 1: Unify Port Definitions into a Single `forge_port` Crate

**Rationale:** Currently, infrastructure abstractions ("ports" in hexagonal architecture) are split between `forge_domain/src/repo.rs` (9 persistence traits) and `forge_app/src/infra.rs` (20+ infrastructure traits) with no consistent principle governing the split. `AgentRepository` lives in `forge_app::infra` while `SkillRepository` lives in `forge_domain::repo`. This makes it impossible to answer "where do I define a new port?" without arbitrary decisions.

### Implementation

- [ ] 1.1 Create a new crate `crates/forge_port` with dependency on `forge_domain` only (for domain types used in trait signatures). This crate will be the **single authoritative location** for all port (trait) definitions.

- [ ] 1.2 Move all 9 repository traits from `crates/forge_domain/src/repo.rs` into `forge_port`:
  - `SnapshotRepository`, `ConversationRepository`, `ChatRepository`, `ProviderRepository`, `WorkspaceIndexRepository`, `SkillRepository`, `PluginRepository`, `ValidationRepository`, `FuzzySearchRepository`

- [ ] 1.3 Move all 20+ infrastructure traits from `crates/forge_app/src/infra.rs` into `forge_port`:
  - `EnvironmentInfra`, `FileReaderInfra`, `FileWriterInfra`, `FileRemoverInfra`, `FileInfoInfra`, `FileDirectoryInfra`, `CommandInfra`, `UserInfra`, `McpClientInfra`, `McpServerInfra`, `WalkerInfra`, `HttpInfra`, `DirectoryReaderInfra`, `KVStore`, `OAuthHttpProvider`, `AuthStrategy`, `StrategyFactory`, `AgentRepository`, `GrpcInfra`, `HookExecutorInfra`, `ElicitationDispatcher`

- [ ] 1.4 Move `ConsoleWriter` from `crates/forge_domain/src/console.rs` into `forge_port` — it is a pure I/O port, not a domain concept.

- [ ] 1.5 Replace `reqwest::Response`, `reqwest::header::HeaderMap`, and `reqwest_eventsource::EventSource` in `HttpInfra` trait signatures with port-owned abstract types (e.g., `HttpResponsePort`, `HeadersPort`, `EventStreamPort`), removing the leakage of HTTP library internals into the port layer. Alternatively, keep the concrete types but re-export them from `forge_port` with clear documentation that these are chosen wire types.

- [ ] 1.6 Update all downstream crates (`forge_app`, `forge_services`, `forge_infra`, `forge_repo`) to import traits from `forge_port` instead of `forge_domain::repo` or `forge_app::infra`.

- [ ] 1.7 In `forge_domain`, remove `repo.rs`, `console.rs`, and all re-exports of moved traits. `forge_domain/src/lib.rs` will no longer contain any trait with async methods or I/O semantics.

### Verification

- `cargo check --workspace` passes with no errors
- All traits previously accessible via `forge_domain::*` or `forge_app::*` are now accessible via `forge_port::*`
- `forge_domain` has zero `async_trait` dependency (domain layer becomes purely synchronous types)
- No grep match for `use forge_domain::.*Repository` or `use forge_app::.*Infra` outside `forge_port`

---

## Phase 2: Decompose the God `Services` Trait into Focused Capability Groups

**Rationale:** The `Services` trait (`crates/forge_app/src/services.rs:735-814`) has 30 associated types and 30 accessor methods. This forces every consumer to carry the entire universe of services even when it needs only one. The ~575 lines of blanket delegation (`crates/forge_app/src/services.rs:816-1390`) exist solely to flatten `services.conversation_service().find_conversation(id)` into `services.find_conversation(id)` — syntactic sugar at enormous boilerplate cost.

### Implementation

- [ ] 2.1 Define **focused capability group traits**, each containing only closely-related service accessors. Candidate grouping:

  | Group Trait | Contains | Used By |
  |---|---|---|
  | `FileServices` | `FsReadService`, `FsWriteService`, `FsPatchService`, `FsRemoveService`, `FsSearchService`, `FsUndoService`, `ImageReadService`, `PlanCreateService` | Tool executor, orch |
  | `ConversationServices` | `ConversationService`, `TemplateService`, `AttachmentService` | Orch, compact |
  | `ProviderServices` | `ProviderService`, `ProviderAuthService`, `AppConfigService` | Orch, agent resolver |
  | `DiscoveryServices` | `FileDiscoveryService`, `CustomInstructionsService`, `WorkspaceService` | System prompt, tool executor |
  | `McpServices` | `McpService`, `McpConfigManager` | Tool executor, orch |
  | `AgentServices` | `AgentRegistry`, `CommandLoaderService`, `SkillFetchService`, `PluginLoader` | Orch, tool resolver |
  | `PolicyServices` | `PolicyService`, `FollowUpService`, `AuthService` | Tool executor, orch |
  | `HookServices` | `HookConfigLoader`, `HookExecutor`, `ElicitationDispatcher` | Lifecycle fires, orch |
  | `ShellServices` | `ShellService`, `NetFetchService` | Tool executor |

- [ ] 2.2 Each group trait follows the same pattern as current `Services` but with 2-8 associated types instead of 30. Each has accessor methods only.

- [ ] 2.3 Define a `Services` supertrait as the union: `trait Services: FileServices + ConversationServices + ProviderServices + DiscoveryServices + McpServices + AgentServices + PolicyServices + HookServices + ShellServices + EnvironmentInfra + Send + Sync + Clone + 'static {}` with a blanket impl `impl<T> Services for T where T: FileServices + ... {}`.

- [ ] 2.4 **Delete all blanket delegation impls** (`crates/forge_app/src/services.rs:816-1390`). Consumers that need `ConversationService` methods use `services.conversation_service().find_conversation(id)` directly. This is explicit, has zero boilerplate, and makes dependency tracking trivial.

- [ ] 2.5 Update every consumer in `forge_app` (orchestrator, tool_executor, system_prompt, hooks, etc.) to bound on the **minimal group trait(s)** they actually need. Example: `tool_executor` bounds on `FileServices + ShellServices + McpServices + PolicyServices` instead of the full `Services`.

- [ ] 2.6 Update `ForgeServices` implementation: instead of one `impl Services for ForgeServices<F>` block with 30 type aliases and 30 methods, implement each group trait separately. The bounds on each `impl` block will be smaller — only the infra traits actually needed by the services in that group.

### Verification

- No single trait in `forge_app` has more than 10 associated types
- `cargo check --workspace` passes
- The `services.rs` file is under 400 lines (from current 1390)
- Every consumer's trait bounds are documented in its function/struct signature — readable at a glance

---

## Phase 3: Eliminate the Triple-Layer Delegation Chain (ForgeInfra / ForgeRepo / ForgeServices)

**Rationale:** Currently the architecture has a single generic `F` parameter threaded through `ForgeServices<ForgeRepo<ForgeInfra>>`. Both `ForgeRepo` and `ForgeInfra` must implement **every** port trait (even those they don't own) via pure delegation to `self.infra`, producing ~500 lines of boilerplate in `ForgeRepo` (`crates/forge_repo/src/forge_repo.rs:226-699`) and ~280 lines in `ForgeInfra` (`crates/forge_infra/src/forge_infra.rs:142-412`). This happens because `ForgeServices<F>` requires `F: AllTraits` as a single composite parameter.

### Implementation

- [ ] 3.1 **Split `ForgeServices` into two type parameters**: `ForgeServices<I, R>` where `I: InfraPort` (file ops, HTTP, commands, walker, grpc, environment, console) and `R: RepoPort` (conversations, snapshots, providers, chat, workspace index, skills, plugins, validation, fuzzy search, KV store). Each parameter requires only its own subset of traits.

- [ ] 3.2 **Remove all passthrough delegation from `ForgeRepo`**: `ForgeRepo` will no longer implement `FileReaderInfra`, `HttpInfra`, `CommandInfra`, `WalkerInfra`, etc. These impls (`crates/forge_repo/src/forge_repo.rs:278-699`) — ~420 lines — are deleted entirely. `ForgeRepo` only implements repository traits (`SnapshotRepository`, `ConversationRepository`, `ChatRepository`, `ProviderRepository`, etc.) that it actually owns.

- [ ] 3.3 **Remove all passthrough delegation from `ForgeInfra`'s aggregator**: `ForgeInfra` struct still holds inner services (`ForgeFileReadService`, `ForgeFileWriteService`, etc.) and implements infra traits by delegating to them. This delegation is kept because it provides the real concrete implementation. However, review and consolidate traits where possible (e.g., merge `FileInfoInfra` + `FileReaderInfra` + `FileWriterInfra` + `FileRemoverInfra` + `FileDirectoryInfra` into a single `FileSystemPort`).

- [ ] 3.4 Refactor concrete service types in `forge_services` to take the specific ports they need. For example, `ForgeFsWrite` should take `Arc<dyn FileWriterInfra + FileInfoInfra + FileDirectoryInfra>` (or a trait alias) instead of `Arc<F>` where `F: 25-trait-bound`. Services that need both infra and repo take `(Arc<I>, Arc<R>)`.

- [ ] 3.5 In `ForgeAPI::init`, wire `ForgeServices::new(Arc<ForgeInfra>, Arc<ForgeRepo<ForgeInfra>>)` with two parameters instead of nesting `ForgeRepo<ForgeInfra>` and passing the whole stack.

- [ ] 3.6 **Consolidate EnvironmentInfra delegation**: Currently `EnvironmentInfra` is implemented at 4 levels (ForgeInfra, ForgeRepo, ForgeServices, and via blanket `Services`). With the split, only `ForgeInfra` implements `EnvironmentInfra`. Services that need it depend on `I: EnvironmentInfra` directly.

### Verification

- `ForgeRepo` is under 300 lines (from 699)
- No trait is implemented by `ForgeRepo` that purely delegates to `self.infra`
- `ForgeServices` struct definition has at most 2 type parameters
- `ConsoleWriter` is implemented only once (in `ForgeInfra`) and passed via the infra parameter
- The total line count of `forge_repo/src/forge_repo.rs` + `forge_infra/src/forge_infra.rs` is < 600 (from ~1111)

---

## Phase 4: Extract Provider DTO Layer from `forge_app` into `forge_repo`

**Rationale:** `crates/forge_app/src/dto/` contains ~135 public types: OpenAI, Anthropic, and Google wire-format request/response DTOs with ~40 transformer implementations. These are infrastructure adapter types used exclusively by provider implementations in `forge_repo`. They have no business in the application layer. Currently `forge_repo` depends on `forge_app` partly because of these DTOs — the dependency arrow is backwards.

### Implementation

- [ ] 4.1 Move the entire `crates/forge_app/src/dto/` directory (except `tools_overview.rs`) into `crates/forge_repo/src/dto/`. This includes:
  - `dto/openai/` (request, response, error, model, reasoning, tool_choice, transformers/*) 
  - `dto/anthropic/` (request, response, error, transforms/*)
  - `dto/google/` (request, response)

- [ ] 4.2 Keep `tools_overview.rs` in `forge_app` since it's an application-level aggregate type not tied to any specific provider.

- [ ] 4.3 Update `forge_repo/Cargo.toml` to include any DTO dependencies currently pulled through `forge_app` (likely already present as `forge_repo` handles provider logic).

- [ ] 4.4 Remove the `pub mod dto;` export from `forge_app/src/lib.rs` (except `tools_overview`). Update `forge_api/src/lib.rs` to stop re-exporting `forge_app::dto::*`.

- [ ] 4.5 Verify that `forge_repo` no longer depends on `forge_app` for DTO types. Audit remaining `forge_repo -> forge_app` dependency edges — if the only remaining reason is port traits, those now come from `forge_port` (Phase 1), potentially making the `forge_repo -> forge_app` dependency eliminable.

### Verification

- `forge_app` has zero files under `src/dto/openai/`, `src/dto/anthropic/`, `src/dto/google/`
- `forge_repo` contains all provider DTO files
- No `use forge_app::dto::` import in `forge_repo` or `forge_infra`
- If `forge_repo -> forge_app` dependency can be eliminated entirely, validate the dependency graph: `forge_services -> forge_app`, `forge_services -> forge_repo`, `forge_services -> forge_port`; but `forge_repo` does NOT depend on `forge_app`

---

## Phase 5: Clean `forge_domain` to Pure Domain Types

**Rationale:** `forge_domain` currently has 63 modules with `pub use *` glob exports, 39 dependencies (including `tokio`, `nom`, `regex`, `serde_yml`, `schemars`), and contains non-domain concerns like `ConsoleWriter` (I/O), `conversation_html.rs` (presentation), `http_config.rs` (infra config), `result_stream_ext.rs` (async stream utils). A domain layer should contain pure business types, value objects, and domain errors — nothing more.

### Implementation

- [ ] 5.1 Remove `ConsoleWriter` (already moved to `forge_port` in Phase 1). Remove `repo.rs` (already moved to `forge_port` in Phase 1).

- [ ] 5.2 Move `conversation_html.rs` (HTML rendering of conversations) to `forge_display` or `forge_app::fmt` — this is presentation logic.

- [ ] 5.3 Move `result_stream_ext.rs` to `forge_stream` — it contains `ResultStreamExt` which extends `Stream` and depends on `tokio`. This is an async utility, not a domain concern.

- [ ] 5.4 Review `http_config.rs`: if it only defines configuration types (structs with Serde derives), it can stay. If it contains HTTP-specific behavior, move to `forge_port` or `forge_infra`.

- [ ] 5.5 Review `template.rs`: if it defines `Template<V>` as a generic value object, it can stay. If it depends on handlebars or rendering logic, move to `forge_app`.

- [ ] 5.6 Review `xml.rs`: if it provides XML generation helpers for prompt formatting, move to `forge_app::system_prompt` or a dedicated formatting module.

- [ ] 5.7 **Replace glob re-exports with explicit module exports**: In `crates/forge_domain/src/lib.rs`, replace every `pub use module::*;` with explicit `pub use module::{Type1, Type2, ...};` — or better, make modules themselves `pub mod` and let consumers use qualified paths. This eliminates namespace pollution (currently ~350+ types in a flat namespace).

- [ ] 5.8 Remove the `pub type ArcSender = tokio::sync::mpsc::Sender<anyhow::Result<ChatResponse>>;` type alias from `forge_domain/src/lib.rs:129` — this is a runtime/infrastructure type alias, not a domain concept.

- [ ] 5.9 Audit `forge_domain`'s Cargo.toml dependencies. After these moves, `tokio` should be removable (or at minimum downgraded to `tokio = { features = [] }` for basic types). The goal is zero async runtime dependency in the domain layer.

### Verification

- `forge_domain` has no `async fn` methods in any public trait
- `forge_domain` does not depend on `tokio` runtime features (only possibly `tokio::sync` for channel types if needed)
- No `pub use module::*` in `forge_domain/src/lib.rs`
- `forge_domain` public API is explicitly listed and documented

---

## Phase 6: Resolve Cyclic Dependencies and OnceLock Late-Init Patterns

**Rationale:** `ForgeAPI::init` (`crates/forge_api/src/forge_api.rs:90-128`) requires three separate post-construction `init_*` calls to resolve circular references between `ForgeServices` and `ForgeInfra` (via `ElicitationDispatcher` and `HookExecutor`). `OnceLock` late-init is a code smell indicating an incorrect dependency graph. The root cause: `ForgeElicitationDispatcher` needs `Arc<ForgeServices>` to fire hooks, but it lives inside `ForgeServices` — creating a self-referential cycle.

### Implementation

- [ ] 6.1 **Extract elicitation dispatching into an event bus pattern**: Define an `ElicitationEventBus` (using `tokio::sync::broadcast` or `mpsc`) that decouples the elicitation trigger (MCP handler in `forge_infra`) from the elicitation consumer (hook pipeline in `forge_services`). The bus is created before any layer, passed into both, and neither needs a reference to the other.

- [ ] 6.2 **Extract hook model service into a callback-based design**: Instead of `ForgeHookExecutor` holding `Arc<dyn HookModelService>` (which is `Arc<ForgeServices>`), inject a `Box<dyn Fn(ModelId, Context) -> Future<Result<String>>>` closure at construction time. The closure is created in `ForgeAPI::init` from the services Arc, but the executor itself doesn't hold a reference to `Services`.

- [ ] 6.3 Remove `init_elicitation_dispatcher()`, `init_hook_executor_services()`, and `init_elicitation_dispatcher(Arc<dyn ElicitationDispatcher>)` from `ForgeServices` and `ForgeInfra`. All wiring happens at construction time without post-init steps.

- [ ] 6.4 Remove the `OnceLock` fields from `ForgeElicitationDispatcher` and `ForgeHookExecutor`. Both receive their dependencies via constructor parameters.

- [ ] 6.5 Simplify `ForgeAPI::init` to a straightforward linear construction sequence without any `init_*` ceremony.

### Verification

- No `OnceLock` usage for dependency injection in `forge_services` or `forge_infra`
- `ForgeAPI::init` contains no `init_*` method calls after initial construction
- `forge_infra` does not depend on `forge_services` (current backwards dependency at `crates/forge_infra/Cargo.toml:16` is eliminated)
- The dependency graph is strictly: `forge_api -> forge_services -> forge_app -> forge_port -> forge_domain`, `forge_api -> forge_repo -> forge_port -> forge_domain`, `forge_api -> forge_infra -> forge_port -> forge_domain` — no arrows pointing upward

---

## Phase 7: Split `forge_repo` by Responsibility

**Rationale:** `forge_repo` currently holds: LLM provider implementations (OpenAI, Anthropic, Bedrock, Vertex, Google — with full SSE/streaming, retry logic, and auth), SQLite persistence (Diesel ORM, migrations, connection pooling), gRPC clients (workspace indexing, validation, fuzzy search), and file-based repositories (agents, skills, plugins from markdown). These are 4 distinct infrastructure concerns with different dependency profiles — the AWS SDK alone adds significant compile time.

### Implementation

- [ ] 7.1 **Create `crates/forge_provider`**: Move all provider-related code:
  - `provider/openai.rs`, `provider/openai_responses/`, `provider/anthropic.rs`, `provider/bedrock.rs`, `provider/google.rs`, `provider/opencode_zen.rs`
  - `provider/event.rs`, `provider/retry.rs`, `provider/chat.rs`, `provider/provider_repo.rs`
  - `provider/bedrock_cache.rs`, `provider/bedrock_sanitize_ids.rs`
  - DTO types moved in Phase 4
  - Dependencies: `async-openai`, `aws-sdk-bedrockruntime`, `aws-credential-types`, `aws-smithy-*`, `google-cloud-auth`, `reqwest`, `reqwest-eventsource`

- [ ] 7.2 **Create `crates/forge_db`**: Move all SQLite persistence:
  - `database/` (pool, schema, migrations)
  - `conversation/` (ConversationRepositoryImpl)
  - Dependencies: `diesel`, `diesel_migrations`

- [ ] 7.3 **Keep `forge_repo`** as a lightweight aggregator that holds `forge_provider`, `forge_db`, and the remaining file-based repos (agents, skills, plugins, snapshots) + gRPC clients. `ForgeRepo` struct still exists but with a much narrower scope — it aggregates actual repositories, not infrastructure passthrough.

- [ ] 7.4 Alternatively, merge the gRPC clients into a `crates/forge_grpc` crate since they all share `tonic`/`prost` dependencies and the generated proto code.

### Verification

- `forge_provider` crate compiles independently with only `forge_domain`, `forge_port`, and external HTTP/LLM dependencies
- `forge_db` crate compiles independently with only `forge_domain` and `diesel`
- `forge_repo` no longer directly depends on `aws-sdk-*`, `diesel`, or `async-openai` — it depends on `forge_provider` and `forge_db`
- Individual provider tests run faster in isolation

---

## Phase 8: Consolidate Redundant File System Traits

**Rationale:** File operations are split across 5 separate traits: `FileReaderInfra`, `FileWriterInfra`, `FileRemoverInfra`, `FileInfoInfra`, `FileDirectoryInfra` (plus `DirectoryReaderInfra`). Each requires separate delegation in every layer. This granularity adds complexity without proportional benefit — in practice, most consumers need read + write + info together.

### Implementation

- [ ] 8.1 Define a unified `FileSystemPort` trait in `forge_port` that combines the 6 file-related traits into one interface. Keep logical grouping via method documentation sections.

- [ ] 8.2 Provide a single implementation `ForgeFileSystem` in `forge_infra` that composes the current `ForgeFileReadService`, `ForgeFileWriteService`, `ForgeFileRemoveService`, `ForgeFileMetaService`, `ForgeCreateDirsService`, `ForgeDirectoryReaderService`.

- [ ] 8.3 Consumers that need only a subset of file ops can still bound on the individual sub-traits if `FileSystemPort` is defined as a supertrait composition: `trait FileSystemPort: FileReaderInfra + FileWriterInfra + FileRemoverInfra + FileInfoInfra + FileDirectoryInfra + DirectoryReaderInfra {}`.

- [ ] 8.4 This consolidation reduces the number of trait impls needed per layer from 6 to 1.

### Verification

- A single `impl FileSystemPort for ForgeInfra` replaces 6 separate `impl` blocks
- No consumer needs to list more than 2 file-related bounds

---

## Phase 9: Clean Up `forge_api` Layer

**Rationale:** `forge_api` defines an `API` trait with 52 methods that largely mirror `Services` methods. It adds value only through: (a) concrete type wiring in `ForgeAPI::init`, (b) lifecycle watcher management, (c) some composite operations (`commit`, `update_config` with cache invalidation). The 52-method trait itself is a maintenance burden.

### Implementation

- [ ] 9.1 **Remove `API` trait**: Make `ForgeAPI<S, F>` a concrete struct with `pub` methods directly. The `API` trait provides no polymorphism benefit — there is exactly one implementation (`ForgeAPI`), and the trait is never used as `dyn API` or as a generic bound outside tests.

- [ ] 9.2 If tests need a mock API, use a focused test trait or expose `ForgeAPI::new(mock_services, mock_infra)` directly.

- [ ] 9.3 **Stop glob re-exporting**: In `forge_api/src/lib.rs`, replace `pub use forge_domain::{Agent, *};` with explicit re-exports of only the types that `forge_main` actually needs. Audit `forge_main` imports to determine the minimal set.

- [ ] 9.4 Move watcher logic into `forge_services` if watchers are a pure services concern, or keep in `forge_api` if they require the fully wired stack. Document the rationale.

### Verification

- No `trait API` definition exists in the codebase
- `forge_api/src/lib.rs` has no `pub use *` statements
- `forge_main` compiles with explicit imports

---

## Phase 10: Eliminate `InfraPluginRepository` Adapter Triplication

**Rationale:** `InfraPluginRepository<F>` is a thin adapter struct created 3 times in `ForgeServices::new` (`crates/forge_services/src/forge_services.rs:163-164`, `192-193`, `210-211`) to convert `Arc<F: PluginRepository>` into `Arc<dyn PluginRepository>`. This exists because some services take `Arc<dyn PluginRepository>` while the generic `F` implements `PluginRepository` but isn't `dyn`-compatible at the point of use.

### Implementation

- [ ] 10.1 Create a single `Arc<dyn PluginRepository>` at the beginning of `ForgeServices::new` and pass the same instance to all three consumers (`ForgeMcpManager`, `ForgeCommandLoaderService`, `ForgeHookConfigLoader`).

- [ ] 10.2 Remove the `InfraPluginRepository` adapter struct entirely — `Arc<F>` can be cast to `Arc<dyn PluginRepository>` directly when `F: PluginRepository + 'static`, using `Arc::clone(&infra) as Arc<dyn PluginRepository>`.

### Verification

- No `InfraPluginRepository` struct exists
- Single `Arc<dyn PluginRepository>` created once and shared

---

## Execution Order and Dependencies

```
Phase 1 (forge_port)         ← Foundation, must go first
  ↓
Phase 5 (clean forge_domain) ← Depends on Phase 1 (traits moved out)
  ↓
Phase 2 (decompose Services) ← Depends on Phase 1 (ports relocated)
  ↓
Phase 3 (eliminate delegation)← Depends on Phase 2 (smaller bounds)
  ↓
Phase 4 (extract DTOs)       ← Independent of Phase 2/3, depends on Phase 1
  ↓
Phase 6 (resolve cycles)     ← Depends on Phase 3 (2 type params)
  ↓
Phase 7 (split forge_repo)   ← Depends on Phase 3 + 4
  ↓
Phase 8 (consolidate FS)     ← Independent, can run after Phase 1
  ↓
Phase 9 (clean forge_api)    ← Depends on Phase 2 (Services decomposed)
  ↓
Phase 10 (cleanup adapter)   ← Trivial, can run anytime
```

**Critical path:** Phase 1 → Phase 5 → Phase 2 → Phase 3 → Phase 6

**Parallelizable:** Phase 4 can run in parallel with Phases 2-3. Phase 8 can run after Phase 1 independently. Phase 10 is standalone.

---

## Potential Risks and Mitigations

1. **Massive merge conflicts during long-running refactoring**
   Mitigation: Execute each phase as a single PR. Never have two phases in-flight simultaneously. Rebase frequently against main.

2. **Breaking test infrastructure (`orch_spec` Runner)**
   Mitigation: The `orch_spec::Runner` test harness implements the full `Services` trait. Phase 2 changes its shape. Update Runner's trait implementations incrementally — it must implement each group trait instead of the monolith `Services`.

3. **Compile time regression from additional crate boundaries**
   Mitigation: More crates = more parallelism in compilation. The provider crate (`forge_provider`) can compile in parallel with `forge_db` and `forge_services`. AWS SDK dependencies are already the bottleneck; isolating them in one crate prevents recompilation when unrelated code changes.

4. **Phase 1 is a large cross-cutting change**
   Mitigation: Use `pub use forge_port::*;` re-exports temporarily in `forge_domain` and `forge_app` during transition, so downstream code doesn't break until explicitly migrated. Remove re-exports only after all consumers are updated.

5. **Service group boundaries may not be optimal**
   Mitigation: The group traits in Phase 2 are soft boundaries. If during implementation a different grouping proves more natural (based on actual usage analysis of each consumer's bounds), adjust. The key invariant is: no group trait has more than 10 associated types.

---

## Alternative Approaches

1. **Dynamic dispatch instead of generics**: Replace `ForgeServices<F>` with `ForgeServices` holding `Arc<dyn PortTrait>` fields. Eliminates all generic bounds explosion but introduces vtable overhead on every service call. Rejected: the codebase explicitly avoids `Box<dyn>` per project guidelines.

2. **Keep `Services` as-is but generate blanket impls with a proc macro**: A `#[service_locator]` macro could auto-generate the delegation boilerplate. Reduces visible code but doesn't fix the underlying coupling. Rejected: hides complexity instead of removing it.

3. **Merge `forge_app` and `forge_services` into one crate**: Since the boundary between them is blurred, merging simplifies the dependency graph. Rejected: separating trait definitions (app) from implementations (services) enables test mocking and enforces interface discipline — it's the right separation, just needs cleaner execution.

4. **Use Ambassador crate for delegation**: The `ambassador` crate can auto-derive trait delegation impls. Would eliminate ~800 lines of boilerplate in ForgeRepo/ForgeInfra. Considered viable as a transitional measure but doesn't fix the root cause (single-parameter F requiring all traits). Can be used as a tool during Phase 3 migration.
