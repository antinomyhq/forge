# Clean Architecture Analysis Report

**Date**: 2026-01-04  
**Project**: Forge  
**Analysis Scope**: Complete codebase architecture review

---

## Executive Summary

This project demonstrates a **strong adherence to clean architecture principles** with a well-structured multi-crate architecture. The dependency flow is correctly directed from outer layers to inner layers, with proper abstraction boundaries and minimal violations.

---

## ✅ Strengths

### 1. **Excellent Layer Separation**

The project is organized into distinct architectural layers:

- **`forge_domain`**: Pure domain layer with entities, value objects, and business logic
- **`forge_app`**: Application layer with use cases, service abstractions, and orchestration
- **`forge_services`**: Service implementations with business logic
- **`forge_infra`**: Infrastructure implementations (file system, HTTP, database)
- **`forge_repo`**: Repository implementations for external integrations
- **`forge_api`**: API/presentation layer that composes everything

**Dependency Flow**: `forge_api` → `forge_services` → `forge_app` → `forge_domain`

This correctly follows clean architecture: **outer layers depend on inner layers, never the reverse**.

### 2. **Strong Dependency Inversion**

The project excellently implements the Dependency Inversion Principle:

**Infrastructure Abstractions** (`forge_app/src/infra.rs`):
- `FileReaderInfra`, `FileWriterInfra`, `CommandInfra`, `HttpInfra`, etc.
- Services depend on these traits, not concrete implementations

**Repository Abstractions** (`forge_domain/src/repo.rs`):
- `ConversationRepository`, `ProviderRepository`, `SnapshotRepository`, etc.
- Defined in domain, implemented in infrastructure

**Service Abstractions** (`forge_app/src/services.rs`):
- `ProviderService`, `ConversationService`, `McpService`, etc.
- Clear contracts with comprehensive trait definitions

### 3. **Service Layer Best Practices**

Services follow the project guidelines excellently:

**Example**: `forge_services/src/provider_service.rs:18-31`
```rust
pub struct ForgeProviderService<R> {
    repository: Arc<R>,
    cached_models: Arc<Mutex<HashMap<ProviderId, Vec<Model>>>>,
}

impl<R> ForgeProviderService<R> {
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            cached_models: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}
```

✅ **Correct patterns**:
- Single generic type parameter
- `Arc<R>` for infrastructure
- Constructor without type bounds
- Type bounds only on methods that need them (`forge_services/src/provider_service.rs:85`)

### 4. **Domain Layer Purity**

The `forge_domain` crate has **minimal external dependencies**:

`forge_domain/Cargo.toml:7-40` shows dependencies are mostly:
- Serialization (`serde`, `serde_json`)
- Utility libraries (`chrono`, `uuid`, `regex`)
- Internal crates (`forge_template`, `forge_json_repair`)

**No infrastructure dependencies** like databases, HTTP clients, or file system access in the domain layer. ✅

### 5. **Proper Infrastructure Composition**

The `ForgeInfra` struct (`forge_infra/src/forge_infra.rs:36-88`) correctly:
- Composes all infrastructure services
- Implements all infrastructure traits by delegating
- Maintains separation of concerns
- Uses dependency injection

---

## ⚠️ Areas for Improvement

### 1. **Domain Layer Has Some Infrastructure Leakage**

**Issue**: `forge_domain` depends on `forge_template` and `forge_json_repair`

`forge_domain/Cargo.toml:30,37`:
```toml
forge_template.workspace = true
forge_json_repair.workspace = true
```

**Impact**: These appear to be utility crates but should be evaluated if they truly belong in the domain layer or if the domain should only define interfaces.

**Recommendation**: 
- If these are pure business logic utilities, they're fine
- If they have I/O or infrastructure concerns, move them to app/services layer and use abstractions

### 2. **Services Layer Depends on Application Layer**

**Issue**: `forge_services` depends on `forge_app`

`forge_services/Cargo.toml:46`:
```toml
forge_app.workspace = true
```

This creates a **circular dependency concern** where:
- `forge_app` defines service abstractions
- `forge_services` implements them
- But `forge_services` also imports from `forge_app`

**Example** (`forge_services/src/provider_service.rs:5-6`):
```rust
use forge_app::ProviderService;
use forge_app::domain::{AnyProvider, ChatCompletionMessage, ...};
```

**Analysis**: This is actually acceptable in this architecture because:
- Services implement traits defined in app layer (correct)
- `forge_app` re-exports `forge_domain` as `forge_app::domain`
- No actual circular dependency exists

**Recommendation**: Consider making the dependency direction clearer by:
- Having `forge_services` import directly from `forge_domain` where possible
- Only importing trait definitions from `forge_app`

### 3. **Large Generic Type Constraints**

**Issue**: The `ForgeServices` struct has very long generic constraints

`forge_services/src/forge_services.rs:45-61`:
```rust
pub struct ForgeServices<
    F: HttpInfra
        + EnvironmentInfra
        + McpServerInfra
        + WalkerInfra
        + SnapshotRepository
        + ConversationRepository
        + AppConfigRepository
        + KVStore
        + ChatRepository
        + ProviderRepository
        + forge_domain::WorkspaceRepository
        + WorkspaceIndexRepository
        + AgentRepository
        + SkillRepository
        + ValidationRepository,
>
```

**Impact**: 
- Hard to read and maintain
- Violates Single Responsibility Principle (SRP) - one type doing too much
- Makes testing harder

**Recommendation**: 
- Split infrastructure into focused groups (e.g., `FileInfra`, `DataInfra`, `AuthInfra`)
- Use trait composition: `trait AllInfra: FileInfra + DataInfra + AuthInfra {}`
- Consider the Facade pattern to hide complexity

### 4. **Repository Pattern Mixing**

**Issue**: Some traits are called "Repository" but are in different layers

- `forge_domain/src/repo.rs` defines repository traits (✅ correct - abstractions in domain)
- `forge_app/src/infra.rs` defines infrastructure traits with "Infra" suffix (✅ correct)
- But `ForgeInfra` implements both repository AND infrastructure traits

`forge_infra/src/forge_infra.rs` - the struct implements:
- `EnvironmentInfra` (infrastructure trait)
- `FileReaderInfra` (infrastructure trait)
- But likely also repository traits indirectly

**Recommendation**:
- Keep naming consistent: use "Repository" for data access, "Infra" for infrastructure
- Consider separating `ForgeInfra` (infrastructure) from `ForgeRepository` (data access)

### 5. **Missing Use Case Layer**

**Observation**: The architecture jumps from services directly to tool execution without explicit use cases

Clean architecture typically has:
```
Controllers → Use Cases → Services → Repositories
```

This project has:
```
Tools → Services → Repositories
```

**Impact**: 
- Business workflows are scattered across services
- Cross-service orchestration logic has no clear home

**Recommendation**: Consider introducing explicit use case classes for complex workflows, such as:
- `ChatWithProviderUseCase` - orchestrates provider selection, auth, and chat
- `SyncWorkspaceUseCase` - coordinates file walking, indexing, and progress tracking

---

## 📊 Dependency Analysis by Crate

| Crate | Layer | Dependencies | Violations |
|-------|-------|-------------|-----------|
| `forge_domain` | Domain | Minimal (serde, chrono, etc.) | ⚠️ Depends on `forge_template`, `forge_json_repair` |
| `forge_app` | Application | `forge_domain` | ✅ None |
| `forge_services` | Services | `forge_domain`, `forge_app` | ⚠️ Minor - imports from forge_app |
| `forge_infra` | Infrastructure | `forge_domain`, `forge_services`, `forge_app` | ✅ None |
| `forge_repo` | Infrastructure | `forge_domain`, `forge_app`, `forge_infra` | ✅ None |
| `forge_api` | Presentation | All layers | ✅ None (outermost layer) |

---

## 🎯 Specific Recommendations

### Priority 1: High Impact

1. **Refactor Generic Constraints**
   ```rust
   // Current
   pub struct ForgeServices<F: HttpInfra + EnvironmentInfra + /* 12 more */>
   
   // Proposed
   pub trait CoreInfra: HttpInfra + EnvironmentInfra + McpServerInfra {}
   pub trait DataInfra: SnapshotRepository + ConversationRepository {}
   pub struct ForgeServices<F: CoreInfra + DataInfra>
   ```

2. **Review Domain Dependencies**
   - Audit `forge_template` and `forge_json_repair` usage in domain
   - Move non-pure logic to application layer
   - Keep domain focused on business rules only

### Priority 2: Medium Impact

3. **Introduce Use Case Layer**
   - Create explicit use case classes for complex workflows
   - Move orchestration logic from services to use cases
   - Keep services focused on single responsibilities

4. **Clarify Naming Conventions**
   - Document distinction between "Repository" vs "Infra" traits
   - Consider renaming for consistency

### Priority 3: Nice to Have

5. **Add Architecture Tests**
   ```rust
   #[test]
   fn domain_has_no_infrastructure_dependencies() {
       // Use cargo metadata to verify forge_domain dependencies
   }
   
   #[test]
   fn services_dont_depend_on_other_services() {
       // Parse service implementations and verify no cross-service deps
   }
   ```

6. **Document Architecture Decisions**
   - Create `docs/architecture.md` explaining layer boundaries
   - Document why certain patterns were chosen
   - Provide examples of where to put new code

---

## 🎓 Testing from Clean Architecture Perspective

The architecture enables **excellent testability**:

**Example**: `forge_services/src/provider_service.rs:169-238`

```rust
struct MockProviderRepository { /* ... */ }

#[async_trait::async_trait]
impl ChatRepository for MockProviderRepository { /* ... */ }

#[async_trait::async_trait]
impl ProviderRepository for MockProviderRepository { /* ... */ }
```

✅ **Strengths**:
- Services can be tested with mock repositories
- No need for real database or HTTP clients
- Business logic tested in isolation
- Tests follow the 3-step pattern (setup, execute, assert)

---

## 📈 Overall Score: **8.5/10**

| Aspect | Score | Notes |
|--------|-------|-------|
| Layer Separation | 9/10 | Excellent crate organization |
| Dependency Direction | 9/10 | Correct inner-to-outer flow |
| Dependency Inversion | 10/10 | Perfect use of traits |
| Domain Purity | 7/10 | Minor infrastructure leakage |
| Service Design | 9/10 | Follows guidelines well |
| Testability | 9/10 | Easy to mock and test |
| Maintainability | 8/10 | Some complexity in generics |

---

## 🎉 Conclusion

This project is a **strong example of clean architecture** in Rust. The team clearly understands and applies the principles effectively. The main areas for improvement are:

1. Reducing generic constraint complexity
2. Ensuring complete domain layer purity
3. Introducing explicit use cases for complex workflows

The architecture is **well-positioned for growth** and should scale effectively as new features are added. The clear boundaries and dependency inversion make it easy to add new implementations, swap infrastructure, and maintain the codebase long-term.

---

## 📋 Appendix: Plan Analysis from Clean Architecture Perspective

### Plan: Dynamic System Context Rendering with Variables
**File**: `plans/2025-04-02-system-context-rendering-final.md`

This plan proposes modifications to the system context rendering mechanism. The following table analyzes each component change from a clean architecture perspective:

| Component | File/Location | Layer | Change Type | Clean Architecture Impact | Risk | Recommendation |
|-----------|--------------|-------|-------------|--------------------------|------|----------------|
| **TemplateService Trait** | `forge_app/src/services.rs` | Application | Signature Change | ✅ **Good** - Trait defined in correct layer (application) | Low | Ensure all implementations are updated |
| **ForgeTemplateService** | `forge_services/src/template.rs` | Services | Implementation Update | ✅ **Good** - Service implementation in correct layer | Low | Watch for infrastructure access (walker, env) |
| **SystemContext** | `forge_domain/src/system_context.rs` | Domain | Data Structure | ⚠️ **Concern** - Adding `variables: HashMap` to domain | Medium | Consider if variables belong in domain or should be app-level |
| **Orchestrator (init_agent)** | `forge_app/src/orch.rs` | Application | Logic Change | ✅ **Good** - Orchestration belongs in app layer | Medium | Re-rendering in loop could impact performance |
| **Orchestrator (init_agent_context)** | `forge_app/src/orch.rs` | Application | Logic Change | ✅ **Good** - Use case orchestration | Low | Clean - passes empty variables initially |
| **Tests** | `forge_services/src/template.rs` | Services | New Tests | ✅ **Good** - Tests at service boundary | Low | Ensure tests use mocks, not real infrastructure |

### Detailed Analysis

#### ✅ Positive Aspects

1. **Correct Layer for Abstractions**
   - The `TemplateService` trait is defined in `forge_app` (application layer) ✅
   - Services implement this trait in `forge_services` ✅
   - Clear separation between interface and implementation

2. **Dependency Direction**
   - Changes flow correctly: App → Services → Domain
   - No reverse dependencies introduced
   - Infrastructure is accessed through abstractions

3. **Consistency with Existing Patterns**
   - Plan notes: "The approach aligns with how event rendering already handles variables"
   - Following established patterns is good for maintainability

#### ⚠️ Concerns

1. **Domain Layer Contamination**
   
   **Issue**: Adding `variables: HashMap<String, Value>` to `SystemContext`
   
   ```rust
   // In forge_domain/src/system_context.rs
   pub struct SystemContext {
       // ... other fields ...
       pub variables: HashMap<String, Value>,  // ⚠️ Generic runtime data
   }
   ```
   
   **Why it's concerning**:
   - Domain entities should represent business concepts
   - `variables` is a generic bag of data without clear business meaning
   - Domain should not know about template rendering concerns
   
   **Alternative Approach**:
   ```rust
   // Option 1: Keep variables at application layer
   // In forge_app or forge_services
   struct TemplateRenderContext {
       system_context: SystemContext,  // Pure domain object
       variables: HashMap<String, Value>,  // App-level concern
   }
   
   // Option 2: Make SystemContext generic if truly needed
   pub struct SystemContext<V = ()> {
       // ... other fields ...
       pub extension: V,  // Type-safe extension point
   }
   ```

2. **Performance Impact**
   
   **Issue**: Re-rendering system context in loop
   
   ```rust
   loop {
       // Re-render on every turn
       let variables = self.conversation.read().await.variables.clone();
       
       if let Some(system_prompt) = &agent.system_prompt {
           let system_message = self.services.template_service()
               .render_system(agent, system_prompt, &variables).await?;
           context = context.set_first_system_message(system_message);
       }
       // ... rest of loop
   }
   ```
   
   **Concerns**:
   - System context rendering involves file walking (`walker.get().await?`)
   - Rendering happens on **every iteration** of the loop
   - Could be expensive if many tool calls in single turn
   
   **Recommendation**:
   - Profile to measure actual impact
   - Consider caching strategy if variables haven't changed
   - Or only re-render when variables are modified

3. **Service Dependencies on Infrastructure**
   
   **Issue**: `ForgeTemplateService` directly accesses infrastructure
   
   ```rust
   async fn render_system(...) -> anyhow::Result<String> {
       let env = self.infra.environment_service().get_environment();
       let mut walker = Walker::max_all();
       let mut files = walker.cwd(env.cwd.clone()).get().await?;
       // ...
   }
   ```
   
   **Analysis**: This is actually acceptable because:
   - Service depends on infrastructure abstractions (correct direction) ✅
   - Infrastructure is injected via `self.infra`
   - Follows dependency inversion principle
   
   **But consider**:
   - File walking on every render might be expensive
   - Could benefit from caching or incremental updates

#### 📊 Change Impact Matrix

| Aspect | Before | After | Clean Architecture Compliance |
|--------|--------|-------|-------------------------------|
| System Prompt Rendering | Once at initialization | Every conversation turn | ✅ Still respects layer boundaries |
| Variable Access | N/A | Passed through layers | ⚠️ Generic data in domain layer |
| Template Service API | `render_system(agent, prompt)` | `render_system(agent, prompt, variables)` | ✅ Interface evolution in correct layer |
| Performance | Render once | Render per turn + file walk | ⚠️ Potential performance concern |
| Testability | Good | Good | ✅ Still mockable |

### Recommendations for Implementation

#### Must Do

1. **Reconsider Variables in Domain**
   ```rust
   // Instead of modifying SystemContext in forge_domain
   // Keep it in the application/service layer
   
   // In forge_services/src/template.rs
   #[derive(Serialize)]
   struct RenderContext {
       #[serde(flatten)]
       system: SystemContext,  // Pure domain object
       variables: HashMap<String, Value>,  // Service-level concern
   }
   ```

2. **Add Performance Tests**
   ```rust
   #[tokio::test]
   async fn test_render_system_performance() {
       let start = Instant::now();
       for _ in 0..100 {
           service.render_system(&agent, &prompt, &variables).await?;
       }
       let elapsed = start.elapsed();
       assert!(elapsed < Duration::from_secs(5), "Rendering too slow");
   }
   ```

#### Should Do

3. **Cache File Walking Results**
   ```rust
   // In ForgeTemplateService
   pub struct ForgeTemplateService<F> {
       infra: Arc<F>,
       file_cache: Arc<Mutex<Option<(Instant, Vec<String>)>>>,
   }
   
   async fn render_system(...) -> Result<String> {
       let files = self.get_cached_files().await?;  // Cache for 1 minute
       // ...
   }
   ```

4. **Document the Trade-off**
   ```rust
   /// Re-renders the system context on every conversation turn.
   /// 
   /// # Performance Considerations
   /// - Involves file system walking on each render
   /// - Variables are cloned from conversation state
   /// - Consider caching if performance becomes an issue
   async fn render_system(...) -> Result<String>
   ```

#### Nice to Have

5. **Consider Incremental Updates**
   ```rust
   // Instead of full re-render, detect what changed
   struct SystemContextDiff {
       variables_changed: bool,
       files_changed: bool,
       time_changed: bool,
   }
   
   // Only re-render affected parts
   ```

### Compliance Summary

| Clean Architecture Principle | Compliance | Notes |
|------------------------------|-----------|-------|
| **Dependency Rule** | ✅ Good | Outer layers depend on inner, not reverse |
| **Interface Segregation** | ✅ Good | Service traits are focused and cohesive |
| **Dependency Inversion** | ✅ Good | Depends on abstractions, not concretions |
| **Single Responsibility** | ⚠️ Mixed | Template service does rendering + file walking |
| **Domain Purity** | ⚠️ Concern | Adding generic variables to domain object |
| **Testability** | ✅ Good | Changes maintain mockability |

### Final Verdict: **7/10 Clean Architecture Compliance**

**Strengths**:
- Maintains proper dependency direction
- Uses abstractions correctly
- Changes are testable
- Follows existing patterns

**Weaknesses**:
- Introduces generic runtime data into domain layer
- Potential performance impact not addressed
- Service has multiple responsibilities (rendering + file I/O)

**Overall**: The plan is implementable and mostly follows clean architecture, but would benefit from reconsidering where `variables` belong in the architecture and addressing performance concerns with caching.
