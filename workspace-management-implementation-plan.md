# Workspace Management Implementation Plan

## 1. Executive Summary

### Problem
Currently, `workspace_id` is a one-way hash derived from folder name, making it impossible to:
- Trace back to original folder when deleted
- Identify orphaned conversation data
- Implement workspace management operations

### Solution
Add a new `workspaces` table to store metadata linking folder paths to workspace_ids with automatic migration and full backward compatibility.

### Benefits
- ✅ Track workspace folder paths
- ✅ Identify orphaned conversations
- ✅ Foundation for workspace CRUD operations
- ✅ Automatic migration with zero downtime
- ✅ Full backward compatibility

## 2. Architecture Overview

### Database Schema
```sql
CREATE TABLE workspaces (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id BIGINT NOT NULL UNIQUE,
    folder_path TEXT NOT NULL UNIQUE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_accessed_at TIMESTAMP NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);
```

### Domain Model
- `Workspace` entity with metadata
- `WorkspaceRepository` trait for operations
- `WorkspaceRepositoryImpl` for SQLite integration
- Automatic workspace tracking on conversation access

### Integration Points
- `ForgeRepo` aggregates `WorkspaceRepository`
- `ConversationRepositoryImpl` updates workspace metadata
- `DatabasePool` handles automatic migration
- Graceful degradation when table doesn't exist

## 3. File Structure Changes

### New Files
```
crates/forge_repo/src/workspace.rs                           # Domain + Repository
crates/forge_repo/src/database/migrations/2025-11-29-000000_add_workspaces_table/up.sql
crates/forge_repo/src/database/migrations/2025-11-29-000000_add_workspaces_table/down.sql
```

### Modified Files
```
crates/forge_repo/src/forge_repo.rs                           # Add WorkspaceRepository
crates/forge_repo/src/conversation.rs                         # Update workspace metadata
crates/forge_repo/src/database/pool.rs                        # Automatic migration
crates/forge_repo/src/database/schema.rs                        # Table definition
crates/forge_repo/src/lib.rs                                  # Module export
crates/forge_domain/src/env.rs                                # current_dir method
```

## 4. Implementation Steps

### 4.1. New Files

#### 4.1.1. `crates/forge_repo/src/workspace.rs`

```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;
use anyhow::Result;
use chrono::{DateTime, Utc};
use forge_domain::{WorkspaceId, MigrationResult};

/// Workspace entity representing a tracked folder
#[derive(Debug, Clone, PartialEq)]
pub struct Workspace {
    pub id: Option<i64>,
    pub workspace_id: WorkspaceId,
    pub folder_path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

/// Database record for workspace storage
struct WorkspaceRecord {
    id: Option<i64>,
    workspace_id: i64,
    folder_path: String,
    created_at: DateTime<Utc>,
    last_accessed_at: Option<DateTime<Utc>>,
    is_active: bool,
}

impl From<Workspace> for WorkspaceRecord {
    fn from(workspace: Workspace) -> Self {
        Self {
            id: workspace.id,
            workspace_id: workspace.workspace_id.id() as i64,
            folder_path: workspace.folder_path.to_string_lossy().to_string(),
            created_at: workspace.created_at,
            last_accessed_at: workspace.last_accessed_at,
            is_active: workspace.is_active,
        }
    }
}

impl WorkspaceRecord {
    fn new(workspace_id: WorkspaceId, folder_path: &Path) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            workspace_id: workspace_id.id() as i64,
            folder_path: folder_path.to_string_lossy().to_string(),
            created_at: now,
            last_accessed_at: Some(now),
            is_active: true,
        }
    }
}

/// Repository trait for workspace operations
pub trait WorkspaceRepository {
    fn create_or_update_workspace(&self, workspace_id: WorkspaceId, folder_path: &Path) -> Result<Workspace>;
    fn get_workspace_by_id(&self, workspace_id: WorkspaceId) -> Result<Option<Workspace>>;
    fn update_last_accessed(&self, workspace_id: WorkspaceId) -> Result<()>;
    fn mark_inactive(&self, workspace_id: WorkspaceId) -> Result<()>;
}

/// SQLite implementation of WorkspaceRepository
pub struct WorkspaceRepositoryImpl {
    pool: Arc<DatabasePool>,
}

impl WorkspaceRepositoryImpl {
    pub fn new(pool: Arc<DatabasePool>) -> Self {
        Self { pool }
    }

    /// Create or update workspace with graceful error handling
    fn try_create_workspace(&self, workspace_id: WorkspaceId, folder_path: &Path) -> Result<Workspace> {
        let mut connection = self.pool.get_connection()?;
        
        let record = WorkspaceRecord::new(workspace_id, folder_path);
        let wid = workspace_id.id() as i64;
        let path = folder_path.to_string_lossy().to_string();

        // Try to insert new workspace
        let result = sqlx::query!(
            r#"
            INSERT OR IGNORE INTO workspaces (workspace_id, folder_path, created_at, last_accessed_at, is_active)
            VALUES (?, ?, ?, ?, ?)
            "#,
            wid,
            path,
            record.created_at,
            record.last_accessed_at,
            record.is_active
        )
        .execute(&mut *connection);

        match result {
            Ok(_) => {
                // New workspace created
                Ok(Workspace {
                    id: None,
                    workspace_id,
                    folder_path: folder_path.to_path_buf(),
                    created_at: record.created_at,
                    last_accessed_at: record.last_accessed_at,
                    is_active: record.is_active,
                })
            }
            Err(_) => {
                // Workspace exists, update it
                sqlx::query!(
                    r#"
                    UPDATE workspaces 
                    SET folder_path = ?, last_accessed_at = ?, is_active = TRUE
                    WHERE workspace_id = ?
                    "#,
                    path,
                    Utc::now(),
                    wid
                )
                .execute(&mut *connection)?;

                // Fetch updated workspace
                let workspace = sqlx::query_as!(
                    Workspace,
                    r#"
                    SELECT id, workspace_id as "workspace_id: i64", folder_path, created_at, last_accessed_at, is_active
                    FROM workspaces 
                    WHERE workspace_id = ?
                    "#,
                    wid
                )
                .fetch_optional(&mut *connection)?;

                Ok(workspace.unwrap_or_else(|| Workspace {
                    id: None,
                    workspace_id,
                    folder_path: folder_path.to_path_buf(),
                    created_at: record.created_at,
                    last_accessed_at: record.last_accessed_at,
                    is_active: record.is_active,
                }))
            }
        }
    }
}

impl WorkspaceRepository for WorkspaceRepositoryImpl {
    fn create_or_update_workspace(&self, workspace_id: WorkspaceId, folder_path: &Path) -> Result<Workspace> {
        match self.try_create_workspace(workspace_id, folder_path) {
            Ok(workspace) => Ok(workspace),
            Err(e) if e.to_string().contains("no such table") || e.to_string().contains("no such table: workspaces") => {
                // Table doesn't exist - return default workspace
                Ok(Workspace {
                    id: None,
                    workspace_id,
                    folder_path: folder_path.to_path_buf(),
                    created_at: Utc::now(),
                    last_accessed_at: Some(Utc::now()),
                    is_active: true,
                })
            },
            Err(e) => Err(e),
        }
    }

    fn get_workspace_by_id(&self, workspace_id: WorkspaceId) -> Result<Option<Workspace>> {
        let mut connection = self.pool.get_connection()?;
        let wid = workspace_id.id() as i64;

        let workspace = sqlx::query_as!(
            Workspace,
            r#"
            SELECT id, workspace_id as "workspace_id: i64", folder_path, created_at, last_accessed_at, is_active
            FROM workspaces 
            WHERE workspace_id = ?
            "#,
            wid
        )
        .fetch_optional(&mut *connection);

        match workspace {
            Ok(Some(ws)) => {
                Ok(Some(Workspace {
                    id: ws.id,
                    workspace_id: WorkspaceId::new(ws.workspace_id as u64),
                    folder_path: PathBuf::from(ws.folder_path),
                    created_at: ws.created_at,
                    last_accessed_at: ws.last_accessed_at,
                    is_active: ws.is_active,
                }))
            }
            Ok(None) => Ok(None),
            Err(e) if e.to_string().contains("no such table") => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn update_last_accessed(&self, workspace_id: WorkspaceId) -> Result<()> {
        let mut connection = self.pool.get_connection()?;
        let wid = workspace_id.id() as i64;

        let result = sqlx::query!(
            r#"
            UPDATE workspaces 
            SET last_accessed_at = ?
            WHERE workspace_id = ?
            "#,
            Utc::now(),
            wid
        )
        .execute(&mut *connection);

        match result {
            Ok(_) => Ok(()),
            Err(e) if e.to_string().contains("no such table") => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn mark_inactive(&self, workspace_id: WorkspaceId) -> Result<()> {
        let mut connection = self.pool.get_connection()?;
        let wid = workspace_id.id() as i64;

        let result = sqlx::query!(
            r#"
            UPDATE workspaces 
            SET is_active = FALSE
            WHERE workspace_id = ?
            "#,
            wid
        )
        .execute(&mut *connection);

        match result {
            Ok(_) => Ok(()),
            Err(e) if e.to_string().contains("no such table") => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use forge_domain::{Environment, WorkspaceId};
    use std::path::PathBuf;

    #[test]
    fn test_workspace_record_creation() {
        let workspace_id = WorkspaceId::new(12345);
        let folder_path = PathBuf::from("/test/path");
        
        let record = WorkspaceRecord::new(workspace_id, &folder_path);
        
        assert_eq!(record.workspace_id, 12345);
        assert_eq!(record.folder_path, "/test/path");
        assert!(record.is_active);
        assert!(record.last_accessed_at.is_some());
    }

    #[test]
    fn test_workspace_from_record() {
        let workspace_id = WorkspaceId::new(12345);
        let folder_path = PathBuf::from("/test/path");
        let record = WorkspaceRecord::new(workspace_id, &folder_path);
        
        let workspace = Workspace {
            id: None,
            workspace_id,
            folder_path,
            created_at: record.created_at,
            last_accessed_at: record.last_accessed_at,
            is_active: true,
        };
        
        let converted_record = WorkspaceRecord::from(workspace.clone());
        assert_eq!(converted_record.workspace_id, 12345);
        assert_eq!(converted_record.folder_path, "/test/path");
    }
}
```

#### 4.1.2. Migration Files

**`crates/forge_repo/src/database/migrations/2025-11-29-000000_add_workspaces_table/up.sql`**

```sql
-- Create workspaces table if it doesn't exist
CREATE TABLE IF NOT EXISTS workspaces (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id BIGINT NOT NULL UNIQUE,
    folder_path TEXT NOT NULL UNIQUE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_accessed_at TIMESTAMP NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);

-- Create indexes if they don't exist
CREATE INDEX IF NOT EXISTS idx_workspaces_workspace_id ON workspaces(workspace_id);
CREATE INDEX IF NOT EXISTS idx_workspaces_folder_path ON workspaces(folder_path);
CREATE INDEX IF NOT EXISTS idx_workspaces_last_accessed ON workspaces(last_accessed_at);

-- Backfill existing workspaces only if they don't exist
INSERT OR IGNORE INTO workspaces (workspace_id, folder_path, created_at, last_accessed_at, is_active)
SELECT 
    c.workspace_id,
    'unknown',
    MIN(c.created_at),
    MAX(c.updated_at),
    FALSE
FROM conversations c
WHERE NOT EXISTS (
    SELECT 1 FROM workspaces w WHERE w.workspace_id = c.workspace_id
)
GROUP BY c.workspace_id;
```

**`crates/forge_repo/src/database/migrations/2025-11-29-000000_add_workspaces_table/down.sql`**

```sql
-- Drop workspaces table (for rollback)
DROP TABLE IF EXISTS workspaces;
```

### 4.2. Modified Files

#### 4.2.1. `crates/forge_repo/src/forge_repo.rs`

**Add import:**
```rust
// Add to existing imports around line 27
use crate::{WorkspaceRepositoryImpl, /* other imports */};
```

**Add field to ForgeRepo struct:**
```rust
// Around line 39, add after conversation_repository
workspace_repository: Arc<WorkspaceRepositoryImpl>,
```

**Update constructor:**
```rust
// Around line 54, after conversation_repository creation
let workspace_repository = Arc::new(WorkspaceRepositoryImpl::new(db_pool.clone()));
```

**Add to struct initialization:**
```rust
// Around line 74, add to Self { ... }
workspace_repository,
```

**Add trait implementation:**
```rust
// Add at the end of file before impl block
#[async_trait::async_trait]
impl<F: Send + Sync> WorkspaceRepository for ForgeRepo<F> {
    fn create_or_update_workspace(&self, workspace_id: WorkspaceId, folder_path: &Path) -> Result<Workspace> {
        self.workspace_repository.create_or_update_workspace(workspace_id, folder_path)
    }

    fn get_workspace_by_id(&self, workspace_id: WorkspaceId) -> Result<Option<Workspace>> {
        self.workspace_repository.get_workspace_by_id(workspace_id)
    }

    fn update_last_accessed(&self, workspace_id: WorkspaceId) -> Result<()> {
        self.workspace_repository.update_last_accessed(workspace_id)
    }

    fn mark_inactive(&self, workspace_id: WorkspaceId) -> Result<()> {
        self.workspace_repository.mark_inactive(workspace_id)
    }
}
```

#### 4.2.2. `crates/forge_repo/src/conversation.rs`

**Add import:**
```rust
// Add to existing imports
use crate::{WorkspaceRepositoryImpl, Workspace};
```

**Add field to ConversationRepositoryImpl:**
```rust
// Around line 174, add to struct
workspace_repository: Arc<WorkspaceRepositoryImpl>,
```

**Update constructor:**
```rust
// Replace lines 179-181 with:
impl ConversationRepositoryImpl {
    pub fn new(pool: Arc<DatabasePool>, workspace_id: WorkspaceId) -> Self {
        let workspace_repository = Arc::new(WorkspaceRepositoryImpl::new(pool.clone()));
        Self { 
            pool, 
            wid: workspace_id,
            workspace_repository,
        }
    }
}
```

**Add helper method:**
```rust
// Add after constructor
impl ConversationRepositoryImpl {
    /// Update workspace access timestamp safely
    fn update_workspace_access(&self) {
        let _ = self.workspace_repository.update_last_accessed(self.wid);
    }

    /// Create workspace record for new conversations
    fn ensure_workspace_exists(&self, folder_path: &Path) {
        let _ = self.workspace_repository.create_or_update_workspace(self.wid, folder_path);
    }
}
```

**Update get_all_conversations method:**
```rust
// Around line 228, add at the beginning:
self.update_workspace_access();
```

**Update get_last_conversation method:**
```rust
// Around line 252, add at the beginning:
self.update_workspace_access();
```

**Update upsert_conversation method:**
```rust
// Around line 186, add at the beginning:
// Note: This requires access to Environment - see forge_repo.rs changes
// For now, we'll update access only
self.update_workspace_access();
```

#### 4.2.3. `crates/forge_repo/src/database/pool.rs`

**Add migration method:**
```rust
// Add to DatabasePool impl
impl DatabasePool {
    pub fn try_from(config: PoolConfig) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect(&config.database_url)
            .await?;

        // Run automatic migrations
        Self::run_migrations(&pool).await?;
        
        Ok(Self { pool })
    }

    async fn run_migrations(pool: &SqlitePool) -> Result<()> {
        // Check if workspaces migration is needed
        let migration_needed = Self::check_workspaces_migration_needed(pool).await?;
        
        if migration_needed {
            Self::run_workspaces_migration(pool).await?;
        }
        
        Ok(())
    }

    async fn check_workspaces_migration_needed(pool: &SqlitePool) -> Result<bool> {
        let result: Option<bool> = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type='table' AND name='workspaces')"
        )
        .fetch_one(pool)
        .await?;
        
        Ok(result.unwrap_or(false) == false)
    }

    async fn run_workspaces_migration(pool: &SqlitePool) -> Result<()> {
        let migration_sql = include_str!("migrations/2025-11-29-000000_add_workspaces_table/up.sql");
        sqlx::raw_sql(migration_sql).execute(pool).await?;
        
        Ok(())
    }
}
```

#### 4.2.4. `crates/forge_repo/src/database/schema.rs`

**Add table definition:**
```rust
// Add to existing table definitions
diesel::table! {
    workspaces (id) {
        id -> Integer,
        workspace_id -> BigInt,
        folder_path -> Text,
        created_at -> Timestamp,
        last_accessed_at -> Nullable<Timestamp>,
        is_active -> Bool,
    }
}
```

#### 4.2.5. `crates/forge_domain/src/env.rs`

**Add method to Environment impl:**
```rust
// Add to Environment impl around line 152
impl Environment {
    // ... existing methods ...

    /// Get current working directory as Path reference
    pub fn current_dir(&self) -> &Path {
        &self.cwd
    }
}
```

#### 4.2.6. `crates/forge_repo/src/lib.rs`

**Add module export:**
```rust
// Add with other module exports
pub mod workspace;
pub use workspace::{Workspace, WorkspaceRepository, WorkspaceRepositoryImpl};
```

## 5. Migration Strategy

### 5.1. Automatic Migration Process

1. **Application Startup**: `DatabasePool::try_from()` checks for workspaces table
2. **Table Creation**: Creates table with `IF NOT EXISTS` for safety
3. **Backfill**: Inserts existing workspace_ids with "unknown" folder path
4. **Index Creation**: Creates performance indexes with `IF NOT EXISTS`
5. **Graceful Handling**: New code works even if table creation fails

### 5.2. Backward Compatibility Guarantees

#### Forward Compatibility (Old → New)
- ✅ Old database → New app: Automatic migration, full functionality
- ✅ Migration is idempotent: Can run multiple times safely
- ✅ No data loss: All existing conversations preserved

#### Backward Compatibility (New → Old)
- ✅ New database → Old app: Old app ignores workspaces table
- ✅ No breaking changes: Old code continues to work
- ✅ Safe rollback: Can downgrade app version anytime

### 5.3. Rollback Procedures

**Immediate Rollback:**
1. Stop application
2. Deploy old version
3. Application ignores workspaces table automatically

**Complete Rollback:**
```sql
-- Remove workspaces table if needed
DROP TABLE IF EXISTS workspaces;
```

## 6. Testing Strategy

### 6.1. Unit Tests

**Workspace Repository Tests:**
- Test creating new workspace
- Test updating existing workspace
- Test retrieving by workspace_id
- Test updating last_accessed_at
- Test marking inactive
- Test graceful handling when table doesn't exist

**Migration Tests:**
- Test table creation
- Test backfill functionality
- Test idempotent migration
- Test index creation

### 6.2. Integration Tests

**Database Integration:**
- Test automatic migration on startup
- Test conversation access updates workspace
- Test backward compatibility scenarios

**Repository Integration:**
- Test ForgeRepo with WorkspaceRepository
- Test ConversationRepository with workspace updates

### 6.3. Compatibility Tests

**Version Compatibility Matrix:**
| DB Version | App Version | Expected Behavior |
|------------|-------------|-------------------|
| Old        | Old         | Works (baseline) |
| Old        | New         | Works + migration |
| New        | Old         | Works (ignores table) |
| New        | New         | Works (full functionality) |

### 6.4. Performance Tests

**Index Performance:**
- Test workspace lookup performance
- Test last_accessed_at updates
- Test query performance with large datasets

**Migration Performance:**
- Test migration time with large conversation tables
- Test backfill performance

## 7. Risk Assessment

### 7.1. Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Migration failure | Low | Medium | IF NOT EXISTS + error handling |
| Performance regression | Low | Low | Optimized indexes |
| Data corruption | Very Low | High | Idempotent operations |
| Backward compatibility issues | Low | High | Graceful degradation |

### 7.2. Operational Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Deployment issues | Medium | Medium | Automatic migration |
| Rollback complexity | Low | Medium | Simple table structure |
| User impact | Low | Medium | Zero-downtime migration |

## 8. Implementation Timeline

### Phase 1: Core Implementation (2-3 days)
- [ ] Create workspace.rs with domain model
- [ ] Implement WorkspaceRepositoryImpl
- [ ] Create migration files
- [ ] Update database schema

### Phase 2: Integration (1-2 days)
- [ ] Update ForgeRepo with WorkspaceRepository
- [ ] Update ConversationRepository with workspace tracking
- [ ] Implement automatic migration in pool.rs
- [ ] Update lib.rs exports

### Phase 3: Testing (1-2 days)
- [ ] Write unit tests for WorkspaceRepository
- [ ] Write integration tests for migration
- [ ] Write compatibility tests
- [ ] Update existing tests

### Phase 4: Documentation & Review (1 day)
- [ ] Update documentation
- [ ] Code review
- [ ] Final testing

**Total Estimated Time: 5-8 days**

## 9. Verification Checklist

### Pre-Implementation
- [ ] Current codebase analyzed and understood
- [ ] Migration strategy approved
- [ ] Test strategy defined
- [ ] Risk assessment completed

### During Implementation
- [ ] All new files created
- [ ] All modifications completed
- [ ] Code compiles without errors
- [ ] Code follows existing patterns

### Post-Implementation
- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] Migration works correctly
- [ ] Backward compatibility verified
- [ ] Performance tests pass
- [ ] Documentation updated

### Pre-Deployment
- [ ] Code reviewed and approved
- [ ] Migration tested on staging
- [ ] Rollback plan verified
- [ ] Monitoring in place

## 10. Success Criteria

### Functional Requirements
- ✅ Workspace table created automatically
- ✅ Existing data migrated safely
- ✅ New workspaces tracked with folder paths
- ✅ Conversation access updates workspace metadata

### Non-Functional Requirements
- ✅ Zero downtime deployment
- ✅ Backward compatibility maintained
- ✅ Performance not degraded
- ✅ Migration is idempotent

### Quality Requirements
- ✅ Code follows existing patterns
- ✅ Tests provide good coverage
- ✅ Documentation is complete
- ✅ Error handling is robust

---

**Ready for Implementation**

This plan provides complete implementation details with full code snippets, migration strategy, testing approach, and risk mitigation. All code is ready to be implemented following the existing codebase patterns and maintaining full backward compatibility.