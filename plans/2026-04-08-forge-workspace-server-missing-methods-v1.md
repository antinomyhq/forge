# Forge Workspace Server — Реализация недостающих методов

## Objective

Реализовать 5 stub-методов, которые Forge CLI реально вызывает, чтобы `:sync`, `:workspace` и другие команды работали с локальным сервером.

## Текущее состояние

| Метод | Статус | Вызывается клиентом? | Критичность |
|-------|--------|---------------------|-------------|
| `CreateApiKey` | Реализован | Да | - |
| `CreateWorkspace` | Реализован | Да | - |
| `UploadFiles` | Реализован | Да | - |
| `ListFiles` | Реализован | Да | - |
| `DeleteFiles` | Реализован | Да | - |
| `Search` | Реализован | Да | - |
| `HealthCheck` | Реализован | Нет (не вызывается) | - |
| **`ListWorkspaces`** | **STUB** | **Да — блокирует :sync** | **P0** |
| **`GetWorkspaceInfo`** | **STUB** | **Да** | **P1** |
| **`DeleteWorkspace`** | **STUB** | **Да** | **P1** |
| **`ValidateFiles`** | **STUB** | **Да (без auth)** | **P2** |
| **`FuzzySearch`** | **STUB** | **Да (без auth)** | **P2** |
| `ChunkFiles` | STUB | Нет | Не нужен |
| `SelectSkill` | STUB | Нет | Не нужен |

## Implementation Plan

### Phase 1: ListWorkspaces (P0 — блокирует :sync)

**Почему P0**: `find_workspace_by_path()` (`crates/forge_services/src/context_engine.rs:141`) вызывает `list_workspaces` при каждом sync для поиска существующего workspace по пути. Без него `:sync` падает сразу.

- [ ] **1.1. Добавить метод `list_workspaces_for_user` в `db.rs`**
  - Сигнатура: `pub async fn list_workspaces_for_user(&self, user_id: &str) -> Result<Vec<WorkspaceRow>>`
  - Запрос: `SELECT workspace_id, working_dir, min_chunk_size, max_chunk_size, created_at FROM workspaces WHERE user_id = ?1`
  - Ввести struct `WorkspaceRow { workspace_id, working_dir, min_chunk_size, max_chunk_size, created_at }` для типизации
  - Также добавить count query для `node_count`: `SELECT COUNT(*) FROM file_refs WHERE workspace_id = ?1` — выполнить для каждого workspace

- [ ] **1.2. Реализовать `list_workspaces` в `server.rs`**
  - Извлечь `user_id` через `authenticate()`
  - Вызвать `db.list_workspaces_for_user(user_id)`
  - Маппинг: каждый `WorkspaceRow` → proto `Workspace { workspace_id, working_dir, node_count, relation_count: 0, min_chunk_size, max_chunk_size, created_at }`
  - `created_at` — парсить timestamp из SQLite и конвертировать в `prost_types::Timestamp`
  - Вернуть `ListWorkspacesResponse { workspaces }`

### Phase 2: GetWorkspaceInfo (P1)

**Почему P1**: Клиент вызывает через `WorkspaceIndexRepository` trait, но на практике `find_workspace_by_path` использует `list_workspaces` + фильтрацию на стороне клиента. Нужен для прямого lookup.

- [ ] **2.1. Добавить метод `get_workspace` в `db.rs`**
  - Сигнатура: `pub async fn get_workspace(&self, workspace_id: &str) -> Result<Option<WorkspaceRow>>`
  - Запрос: `SELECT workspace_id, working_dir, min_chunk_size, max_chunk_size, created_at FROM workspaces WHERE workspace_id = ?1`

- [ ] **2.2. Реализовать `get_workspace_info` в `server.rs`**
  - Извлечь `user_id` через `authenticate()`
  - Извлечь `workspace_id` из request
  - Вызвать `db.get_workspace(workspace_id)`
  - Если workspace не найден — вернуть `GetWorkspaceInfoResponse { workspace: None }`
  - Если найден — маппинг аналогичен Phase 1 + `node_count` из `file_refs`

### Phase 3: DeleteWorkspace (P1)

**Почему P1**: Forge CLI вызывает при `:workspace delete`. Также используется в `delete_workspaces()` для batch-удаления.

- [ ] **3.1. Добавить метод `delete_workspace` в `db.rs`**
  - Сигнатура: `pub async fn delete_workspace(&self, workspace_id: &str) -> Result<()>`
  - Порядок: `DELETE FROM file_refs WHERE workspace_id = ?1`, затем `DELETE FROM workspaces WHERE workspace_id = ?1`
  - Foreign key cascade не используется — удаляем явно в правильном порядке

- [ ] **3.2. Реализовать `delete_workspace` в `server.rs`**
  - Извлечь `user_id` через `authenticate()`
  - Извлечь `workspace_id` из request
  - Удалить коллекцию в Qdrant: `qdrant.delete_collection(workspace_id)`
  - Удалить метаданные в SQLite: `db.delete_workspace(workspace_id)`
  - Вернуть `DeleteWorkspaceResponse { workspace_id }`

### Phase 4: ValidateFiles (P2)

**Почему P2**: Клиент вызывает для проверки синтаксиса после записи файлов. Без auth. Graceful degradation: клиент обработает ошибку и продолжит работу.

- [ ] **4.1. Реализовать `validate_files` в `server.rs` — stub с `UnsupportedLanguage`**
  - Для MVP: вернуть `UnsupportedLanguage` для всех файлов
  - Клиент при получении `UnsupportedLanguage` просто скипает валидацию — `crates/forge_repo/src/validation.rs:103-114`
  - Это означает что validate просто не будет работать, но ничего не сломается
  - Response: `ValidateFilesResponse { results: [FileValidationResult { file_path, status: UnsupportedLanguage }] }`

**Follow-up (не MVP)**: интеграция с tree-sitter для реальной синтаксической валидации

### Phase 5: FuzzySearch (P2)

**Почему P2**: Клиент вызывает для неточного поиска (needle in haystack). Без auth. Используется при `:search` и инструментах.

- [ ] **5.1. Реализовать `fuzzy_search` в `server.rs` — простая substring-реализация**
  - Для MVP: split `haystack` по строкам, найти строки содержащие `needle` (case-insensitive)
  - Если `search_all = false` — вернуть только первое совпадение
  - Если `search_all = true` — все совпадения
  - Response: `FuzzySearchResponse { matches: [SearchMatch { start_line, end_line }] }`
  - `start_line` и `end_line` — 1-based номера строк

**Follow-up (не MVP)**: настоящий fuzzy matching (Levenshtein, Smith-Waterman или аналог)

## Вспомогательные изменения

- [ ] **6.1. Утилита парсинга timestamp в `server.rs`**
  - Сейчас `chrono_now()` в `db.rs:217-224` сохраняет timestamp как unix seconds string
  - Нужна функция `parse_timestamp(s: &str) -> Option<prost_types::Timestamp>` для конвертации обратно
  - Использовать в `ListWorkspaces` и `GetWorkspaceInfo`

- [ ] **6.2. Убрать `#[allow(dead_code)]` / warning для `delete_collection`**
  - После Phase 3 метод `qdrant.delete_collection` будет использоваться — warning уйдёт сам

## Файлы для изменения

| Файл | Изменения |
|------|-----------|
| `server/src/db.rs` | + struct `WorkspaceRow`, + `list_workspaces_for_user`, + `get_workspace`, + `delete_workspace`, + `count_file_refs` |
| `server/src/server.rs` | Заменить 5 stubs на реализации, + `parse_timestamp` helper |

**Файлы без изменений**: `config.rs`, `auth.rs`, `embedder.rs`, `chunker.rs`, `qdrant.rs`, `main.rs`, `build.rs`, `Cargo.toml`, `proto/forge.proto`

## Verification Criteria

- `cargo check` — 0 errors
- `cargo test` — все существующие тесты проходят
- `:sync` в Forge CLI — проходит без ошибок, файлы загружаются
- `grpcurl` тест `ListWorkspaces` — возвращает ранее созданные workspaces
- `grpcurl` тест `DeleteWorkspace` — удаляет workspace, повторный `ListWorkspaces` его не содержит
- `grpcurl` тест `ValidateFiles` — возвращает `UnsupportedLanguage` для любого файла
- `grpcurl` тест `FuzzySearch` — находит подстроку в тексте

## Potential Risks and Mitigations

1. **`ListWorkspaces` возвращает workspaces всех пользователей**
   Mitigation: фильтровать по `user_id`, извлечённому из Bearer token через `authenticate()`

2. **`node_count` требует JOIN или отдельного запроса**
   Mitigation: для MVP — отдельный COUNT query на каждый workspace. При большом количестве workspaces (>100) можно оптимизировать через LEFT JOIN

3. **`created_at` формат в SQLite не парсится**
   Mitigation: сейчас `chrono_now()` сохраняет unix seconds как строку. `parse_timestamp` должен обрабатывать оба формата: unix seconds и ISO 8601 (на случай если формат изменится)

4. **`FuzzySearch` простой substring не покрывает все use-cases**
   Mitigation: MVP достаточен — клиент корректно обработает результаты. Fuzzy matching можно добавить позже

## Порядок реализации

```
Phase 1 (ListWorkspaces) → Phase 6.1 (timestamp) → Phase 2 (GetWorkspaceInfo) → Phase 3 (DeleteWorkspace) → Phase 4 (ValidateFiles) → Phase 5 (FuzzySearch)
```

Phase 1 разблокирует `:sync`. Остальные можно делать в любом порядке.
