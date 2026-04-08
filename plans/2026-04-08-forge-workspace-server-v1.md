# Forge Workspace Server — Self-hosted Rust Implementation

## Objective

Implement a self-hosted gRPC server in Rust that is fully compatible with the existing Forge CLI client (`forge_repo/src/context_engine.rs`). The server indexes codebases into a vector database (Qdrant) using locally-generated embeddings (Ollama + `nomic-embed-text`) and serves semantic search queries. It must implement the 7 MVP methods from `forge.proto` that the client actually calls.

## Architecture Overview

```
┌──────────────┐    gRPC (proto)    ┌──────────────────┐
│  Forge CLI   │ ◄───────────────► │  Workspace Server │
│  (existing)  │   Bearer token     │  (this project)   │
└──────────────┘                    └────────┬──────────┘
                                             │
                              ┌──────────────┼──────────────┐
                              │              │              │
                         ┌────▼────┐   ┌─────▼─────┐  ┌────▼────┐
                         │ Qdrant  │   │  Ollama   │  │ SQLite  │
                         │ vectors │   │ embeddings│  │ metadata│
                         └─────────┘   └───────────┘  └─────────┘
```

**Data flow summary:**
1. `CreateApiKey` → generate UUID user_id + random token, store in SQLite
2. `CreateWorkspace` → generate UUID workspace_id, create Qdrant collection `ws_{id}`, store mapping in SQLite
3. `UploadFiles` → chunk each file → embed chunks via Ollama → upsert points into Qdrant collection
4. `ListFiles` → query Qdrant for all `FileRef` nodes in collection, return `{path, hash}` pairs
5. `DeleteFiles` → delete Qdrant points by `file_path` filter
6. `Search` → embed query via Ollama → ANN search in Qdrant → return scored `FileChunk` nodes
7. `HealthCheck` → return `"ok"`

---

## Proto Contract Analysis

Source: `crates/forge_repo/proto/forge.proto`

### Methods to implement (MVP)

| # | RPC Method | Request | Response | Client callsite |
|---|-----------|---------|----------|----------------|
| 1 | `CreateApiKey` | `CreateApiKeyRequest { user_id: optional }` | `CreateApiKeyResponse { user_id, key }` | `context_engine.rs:118-130` |
| 2 | `CreateWorkspace` | `CreateWorkspaceRequest { workspace: WorkspaceDefinition { working_dir, min_chunk_size, max_chunk_size } }` | `CreateWorkspaceResponse { workspace: Workspace }` | `context_engine.rs:132-151` |
| 3 | `UploadFiles` | `UploadFilesRequest { workspace_id, content: FileUploadContent { files: [File { path, content }], git } }` | `UploadFilesResponse { result: UploadResult { node_ids, relations } }` | `context_engine.rs:153-189` |
| 4 | `ListFiles` | `ListFilesRequest { workspace_id }` | `ListFilesResponse { files: [FileRefNode { node_id, hash, git, data: FileRef { path, file_hash } }] }` | `context_engine.rs:317-341` |
| 5 | `DeleteFiles` | `DeleteFilesRequest { workspace_id, file_paths }` | `DeleteFilesResponse { deleted_nodes, deleted_relations }` | `context_engine.rs:344-367` |
| 6 | `Search` | `SearchRequest { workspace_id, query: Query { prompt, limit, top_k, relevance_query, kinds, starts_with, ends_with } }` | `SearchResponse { result: QueryResult { data: [QueryItem { node, distance, rank, relevance }] } }` | `context_engine.rs:192-276` |
| 7 | `HealthCheck` | `HealthCheckRequest {}` | `HealthCheckResponse { status }` | Not called by client directly, useful for ops |

### Methods NOT needed (stub or skip)

`GetWorkspaceInfo`, `ListWorkspaces`, `DeleteWorkspace`, `ChunkFiles`, `ValidateFiles`, `SelectSkill`, `FuzzySearch` — these are either not called by the sync/search flow or can return `UNIMPLEMENTED`.

### Authentication contract

The client sets Bearer token via gRPC metadata (`context_engine.rs:103-113`):
```
authorization: Bearer <token_string>
```
The server must extract and validate this header on all methods except `CreateApiKey` (which is the bootstrap call, `context_engine.rs:121` — no auth header sent).

### Key data types from client

- **WorkspaceId**: UUID string (`workspace.rs:10` — `Uuid` wrapped in newtype)
- **UserId**: UUID string (`node.rs:174` — `Uuid` wrapped in newtype)  
- **ApiKey**: opaque string (`new_types.rs:7` — String newtype, sent as Bearer)
- **FileHash**: `{ path: String, hash: String }` where hash is SHA-256 hex (`file.rs:43-48`, computed via `sha2` at `utils.rs:103-108`)
- **FileChunk**: `{ path: String, content: String, start_line: u32, end_line: u32 }` (`node.rs:354-363`)
- **SearchParams**: `{ query, limit, top_k, use_case (= relevance_query), starts_with, ends_with }` (`node.rs:142-149`)

### Client search behavior (critical for compatibility)

From `context_engine.rs:197-211`:
- Client always requests `kinds: [NODE_KIND_FILE_CHUNK]`
- `prompt` and `relevance_query` are always set
- `starts_with` may contain a directory prefix filter
- `ends_with` may contain file extension filters (e.g., `[".rs", ".ts"]`)
- `max_distance` is always `None`
- `limit` and `top_k` are optional

From `context_engine.rs:225-273`, the client expects:
- `QueryItem.node.data` to be `FileChunk` variant (line 236)
- `QueryItem.relevance` and `QueryItem.distance` as optional floats
- `node.node_id` is expected but defaults to empty string if missing

### Client upload behavior

From `sync.rs:282-312`:
- Files are uploaded **one at a time** (each `UploadFilesRequest` contains exactly 1 `File`)
- Uploads are parallelized client-side via `buffer_unordered(batch_size)`
- The server must handle concurrent uploads to the same workspace

### Client ListFiles behavior

From `context_engine.rs:317-341` and `workspace_status.rs:50-92`:
- Returns `FileRefNode` with `{ path, file_hash }` (the SHA-256 hex of the original content)
- Client compares local SHA-256 hashes against these to detect new/modified/deleted files
- **Critical**: the `hash` field in `FileRefNode` must match `sha2::Sha256` hex of the original file content (same algorithm as `compute_hash` at `crates/forge_app/src/utils.rs:103-108`)

---

## Implementation Plan

### Phase 0: Project Setup

- [ ] 0.1. **Create project directory** — Initialize a new Rust project (e.g., `forge-workspace-server/`) with `cargo init`. Structure:
  ```
  forge-workspace-server/
  ├── proto/
  │   └── forge.proto          # Copy from crates/forge_repo/proto/forge.proto
  ├── src/
  │   ├── main.rs              # Entry point: CLI args, tokio runtime, server startup
  │   ├── server.rs            # ForgeService tonic impl (all RPC handlers)
  │   ├── auth.rs              # Token validation middleware / extractor
  │   ├── chunker.rs           # File → FileChunk splitting logic
  │   ├── embedder.rs          # Ollama HTTP client for embedding generation
  │   ├── qdrant.rs            # Qdrant client wrapper (collection management, upsert, search, delete)
  │   ├── db.rs                # SQLite metadata storage (users, workspaces, api_keys)
  │   └── config.rs            # Server configuration (CLI args, env vars)
  ├── build.rs                 # tonic-build proto compilation
  ├── Cargo.toml
  └── README.md
  ```

- [ ] 0.2. **Set up `Cargo.toml` dependencies**:
  - `tonic = "0.12"` — gRPC server framework
  - `tonic-build = "0.12"` (build-dep) — proto code generation  
  - `prost = "0.13"` + `prost-types = "0.13"` — protobuf types
  - `tokio = { version = "1", features = ["full"] }` — async runtime
  - `qdrant-client = "1"` — Qdrant vector DB client
  - `reqwest = { version = "0.12", features = ["json"] }` — HTTP client for Ollama API
  - `rusqlite = { version = "0.31", features = ["bundled"] }` — SQLite for metadata
  - `uuid = { version = "1", features = ["v4"] }` — UUID generation
  - `sha2 = "0.10"` + `hex = "0.4"` — SHA-256 hashing (must match client's `compute_hash`)
  - `serde = { version = "1", features = ["derive"] }` + `serde_json = "1"` — serialization
  - `anyhow = "1"` — error handling
  - `tracing = "0.1"` + `tracing-subscriber = "0.3"` — structured logging
  - `clap = { version = "4", features = ["derive"] }` — CLI argument parsing

- [ ] 0.3. **Set up `build.rs`** for tonic-build:
  - Compile `proto/forge.proto` with `tonic_build::configure().build_server(true).build_client(false)`
  - Disable client codegen (we only need the server side)
  - Handle the `google/protobuf/timestamp.proto` dependency (tonic-build bundles well-known types via `prost-types`)

- [ ] 0.4. **Copy `forge.proto`** from `crates/forge_repo/proto/forge.proto` into `proto/forge.proto`

### Phase 1: Configuration & Entry Point

- [ ] 1.1. **Implement `config.rs`** — Server configuration struct with defaults:
  - `listen_addr`: gRPC listen address (default `0.0.0.0:50051`)
  - `qdrant_url`: Qdrant gRPC endpoint (default `http://localhost:6334`)
  - `ollama_url`: Ollama HTTP endpoint (default `http://localhost:11434`)
  - `embedding_model`: model name (default `nomic-embed-text`)
  - `embedding_dim`: vector dimension (default `768` for nomic-embed-text)
  - `db_path`: SQLite file path (default `./forge-server.db`)
  - `chunk_max_size`: default max chunk size in bytes (default `1500`)
  - `chunk_min_size`: default min chunk size in bytes (default `100`)
  - Parse from CLI args via `clap` + override from env vars

- [ ] 1.2. **Implement `main.rs`** — Entry point:
  - Parse config
  - Initialize tracing subscriber
  - Initialize SQLite database (run migrations)
  - Create Qdrant client connection
  - Verify Ollama is reachable (health check request)
  - Build `ForgeServiceImpl` with all dependencies
  - Start tonic gRPC server on `listen_addr`
  - Log startup message with all endpoint URLs

### Phase 2: SQLite Metadata Storage (`db.rs`)

- [ ] 2.1. **Design SQLite schema** — Three tables:
  ```sql
  CREATE TABLE IF NOT EXISTS api_keys (
      key TEXT PRIMARY KEY,
      user_id TEXT NOT NULL,
      created_at TEXT NOT NULL DEFAULT (datetime('now'))
  );

  CREATE TABLE IF NOT EXISTS workspaces (
      workspace_id TEXT PRIMARY KEY,
      user_id TEXT NOT NULL,
      working_dir TEXT NOT NULL,
      min_chunk_size INTEGER NOT NULL DEFAULT 100,
      max_chunk_size INTEGER NOT NULL DEFAULT 1500,
      created_at TEXT NOT NULL DEFAULT (datetime('now')),
      UNIQUE(user_id, working_dir)
  );

  CREATE TABLE IF NOT EXISTS file_refs (
      workspace_id TEXT NOT NULL,
      file_path TEXT NOT NULL,
      file_hash TEXT NOT NULL,
      node_id TEXT NOT NULL,
      updated_at TEXT NOT NULL DEFAULT (datetime('now')),
      PRIMARY KEY (workspace_id, file_path),
      FOREIGN KEY (workspace_id) REFERENCES workspaces(workspace_id)
  );
  ```

  **Rationale**: `file_refs` stores the original SHA-256 content hash per file (used by `ListFiles`) and the mapping to Qdrant node IDs. The `UNIQUE(user_id, working_dir)` constraint on `workspaces` supports the idempotent "create or get" pattern that the client uses during sync.

- [ ] 2.2. **Implement `Database` struct** wrapping `rusqlite::Connection`:
  - `new(path) -> Result<Self>` — open/create DB, run migrations
  - `create_api_key(user_id: Option<&str>) -> Result<(String, String)>` — returns `(user_id, key)`
  - `validate_api_key(key: &str) -> Result<Option<String>>` — returns `user_id` if valid
  - `create_workspace(user_id, working_dir, min_chunk, max_chunk) -> Result<Workspace proto>`
  - `get_workspace_by_working_dir(user_id, working_dir) -> Result<Option<Workspace>>`
  - `upsert_file_ref(workspace_id, file_path, file_hash, node_id) -> Result<()>`
  - `delete_file_refs(workspace_id, file_paths) -> Result<usize>`
  - `list_file_refs(workspace_id) -> Result<Vec<FileRefNode proto>>`

  Use `tokio::task::spawn_blocking` for all SQLite calls since rusqlite is synchronous.

### Phase 3: Ollama Embedding Client (`embedder.rs`)

- [ ] 3.1. **Implement `Embedder` struct**:
  - Stores `reqwest::Client`, `ollama_url`, `model_name`, `embedding_dim`
  - `new(ollama_url, model, dim) -> Self`
  - `embed_single(text: &str) -> Result<Vec<f32>>` — POST to `{ollama_url}/api/embed` with `{"model": model, "input": text}`, parse response `{"embeddings": [[f32; dim]]}`, return the first (and only) embedding vector
  - `embed_batch(texts: &[String]) -> Result<Vec<Vec<f32>>>` — POST to `{ollama_url}/api/embed` with `{"model": model, "input": texts}`, parse all embeddings. Ollama's `/api/embed` endpoint supports batch input natively.
  - `health_check() -> Result<()>` — GET `{ollama_url}/` to verify Ollama is running

  **Ollama API contract** (`POST /api/embed`):
  ```json
  Request:  {"model": "nomic-embed-text", "input": ["text1", "text2"]}
  Response: {"model": "nomic-embed-text", "embeddings": [[0.1, 0.2, ...], [0.3, 0.4, ...]]}
  ```

### Phase 4: File Chunking (`chunker.rs`)

- [ ] 4.1. **Implement line-aware chunker** — Split file content into chunks with line-number tracking:
  - `chunk_file(path: &str, content: &str, min_size: u32, max_size: u32) -> Vec<ChunkResult>`
  - `ChunkResult { path: String, content: String, start_line: u32, end_line: u32 }`
  - Algorithm:
    1. Split content by lines
    2. Accumulate lines into a chunk until byte size exceeds `max_size`
    3. When a chunk is full, finalize it and start a new one
    4. If the last chunk is smaller than `min_size`, merge it with the previous chunk
    5. Track `start_line` (1-based) and `end_line` (inclusive) for each chunk
  - Edge cases: empty files produce 0 chunks, files smaller than `min_size` produce 1 chunk

  **Rationale**: The client expects `FileChunk { path, content, start_line, end_line }` in search results. Line-based splitting preserves code structure better than byte-offset splitting. The `min_chunk_size` and `max_chunk_size` come from `WorkspaceDefinition` in the proto (or server defaults).

### Phase 5: Qdrant Integration (`qdrant.rs`)

- [ ] 5.1. **Implement `QdrantStore` struct**:
  - Stores `qdrant_client::Qdrant` client and `embedding_dim`
  - `new(qdrant_url, embedding_dim) -> Result<Self>`

- [ ] 5.2. **Collection management**:
  - `ensure_collection(workspace_id: &str) -> Result<()>` — create collection `ws_{workspace_id}` if not exists, with cosine distance and configured vector dimension
  - `delete_collection(workspace_id: &str) -> Result<()>` — for workspace deletion

- [ ] 5.3. **Upsert points (used by UploadFiles)**:
  - `upsert_chunks(workspace_id: &str, chunks: Vec<ChunkPoint>) -> Result<Vec<String>>` where:
    ```
    ChunkPoint {
      id: String (UUID),
      vector: Vec<f32>,
      file_path: String,
      content: String,
      start_line: u32,
      end_line: u32,
    }
    ```
  - Each point's payload: `{ "file_path": String, "content": String, "start_line": u32, "end_line": u32, "node_kind": "file_chunk" }`
  - Returns the generated point UUIDs (used as `node_ids` in the response)

- [ ] 5.4. **Delete by file path (used by DeleteFiles and re-upload)**:
  - `delete_by_file_paths(workspace_id: &str, paths: &[String]) -> Result<u32>` — delete all points where `file_path` matches any of the given paths. Returns count of deleted points.
  
  **Critical**: When a file is re-uploaded (modified), the server must first delete all old chunks for that file path, then insert new chunks. This prevents stale chunks from accumulating.

- [ ] 5.5. **Search (used by Search RPC)**:
  - `search(workspace_id: &str, vector: Vec<f32>, limit: u32, filter: Option<QdrantFilter>) -> Result<Vec<SearchHit>>`
  - `SearchHit { id: String, score: f32, file_path: String, content: String, start_line: u32, end_line: u32 }`
  - Build Qdrant filter from `starts_with` (prefix match on `file_path`) and `ends_with` (suffix match on `file_path`)
  - Use `SearchPoints` with `with_payload: true`

- [ ] 5.6. **Scroll all points (used by ListFiles — optional optimization)**:
  - Alternative: rely on SQLite `file_refs` table for `ListFiles` instead of scrolling Qdrant. This is faster and more reliable.
  - Decision: **Use SQLite** for `ListFiles` since the client only needs `{path, hash}` pairs, not vector data.

### Phase 6: Authentication Middleware (`auth.rs`)

- [ ] 6.1. **Implement auth interceptor/extractor**:
  - Extract `authorization` metadata from gRPC request
  - Parse `Bearer <token>` format
  - Look up token in SQLite `api_keys` table
  - Return `user_id` on success, `tonic::Status::unauthenticated` on failure
  - Skip auth for `CreateApiKey` (bootstrap method — the client calls it without auth, as seen in `context_engine.rs:121`)
  
  Implementation approach: use a helper function called from each RPC handler rather than a tonic interceptor, since `CreateApiKey` must be exempt. Signature:
  ```
  async fn authenticate(db: &Database, request: &tonic::Request<T>) -> Result<String, tonic::Status>
  ```

### Phase 7: gRPC Service Implementation (`server.rs`)

- [ ] 7.1. **Define `ForgeServiceImpl` struct**:
  ```rust
  pub struct ForgeServiceImpl {
      db: Arc<Database>,
      qdrant: Arc<QdrantStore>,
      embedder: Arc<Embedder>,
      config: Arc<Config>,
  }
  ```

- [ ] 7.2. **Implement `CreateApiKey`**:
  - If `user_id` is provided, use it; otherwise generate UUID v4
  - Generate random API key (e.g., `uuid::Uuid::new_v4().to_string()` or a longer random string)
  - Store `(key, user_id)` in SQLite
  - Return `CreateApiKeyResponse { user_id: Some(UserId { id }), key }`
  - No auth required on this method

- [ ] 7.3. **Implement `CreateWorkspace`**:
  - Authenticate request
  - Extract `working_dir` from `WorkspaceDefinition`
  - Check if workspace already exists for this `(user_id, working_dir)` — if yes, return existing
  - Otherwise: generate UUID workspace_id, create Qdrant collection, insert into SQLite
  - Return full `Workspace` message with `workspace_id`, `working_dir`, `created_at`, `node_count: 0`, etc.
  
  **Idempotency note**: The client calls `CreateWorkspace` on every sync. If a workspace already exists for the same `working_dir`, return the existing one (this is the pattern the production server uses, evidenced by the `UNIQUE(user_id, working_dir)` constraint and the client's `find_workspace_by_path` fallback in `context_engine_service.rs`).

- [ ] 7.4. **Implement `UploadFiles`**:
  - Authenticate request
  - Extract `workspace_id` and `files` from request
  - For each file in `content.files`:
    1. Compute SHA-256 hash of `file.content` (must match client's `compute_hash`)
    2. Delete existing chunks for this `file.path` in Qdrant (handles re-uploads)
    3. Split `file.content` into chunks using `chunker::chunk_file`
    4. Embed all chunks via `embedder.embed_batch`
    5. Upsert chunk vectors + payloads into Qdrant
    6. Upsert `file_refs` entry in SQLite with `(workspace_id, file.path, sha256_hash, node_id)`
  - Return `UploadResult { node_ids: [all chunk UUIDs], relations: [] }`
  
  **Concurrency**: The client sends files in parallel (`buffer_unordered`). The server must handle concurrent `UploadFiles` calls safely. SQLite operations are serialized naturally (single writer), and Qdrant handles concurrent writes. No explicit locking needed beyond what rusqlite provides.

- [ ] 7.5. **Implement `ListFiles`**:
  - Authenticate request
  - Query SQLite `file_refs` table for all entries matching `workspace_id`
  - Map each row to `FileRefNode { node_id, hash, git: None, data: FileRef { path, file_hash } }`
  - Return `ListFilesResponse { files }`

- [ ] 7.6. **Implement `DeleteFiles`**:
  - Authenticate request
  - Extract `workspace_id` and `file_paths`
  - Delete Qdrant points by `file_path` filter for each path
  - Delete SQLite `file_refs` entries
  - Return `DeleteFilesResponse { deleted_nodes: count, deleted_relations: 0 }`

- [ ] 7.7. **Implement `Search`**:
  - Authenticate request
  - Extract `workspace_id`, `query.prompt`, `query.top_k` (default 10), `query.limit`
  - Embed `query.prompt` via `embedder.embed_single`
  - Build Qdrant filter from `query.starts_with` (file path prefix) and `query.ends_with` (file extension suffix)
  - Execute ANN search in Qdrant collection `ws_{workspace_id}`
  - Map results to `QueryItem` with:
    - `node.data = FileChunk { path, content, start_line, end_line }`
    - `node.node_id = point UUID`
    - `node.workspace_id = workspace_id`
    - `node.hash = ""` (not used for chunks)
    - `distance = Some(1.0 - score)` (Qdrant cosine returns similarity 0..1, client expects distance)
    - `relevance = Some(score)`
  - **Reranking with `relevance_query`**: For MVP, use the same embedding score as relevance. Reranking with a cross-encoder model can be added later as an enhancement.
  - Apply `limit` to cap the number of returned results
  - Return `SearchResponse { result: QueryResult { data: items } }`

- [ ] 7.8. **Implement `HealthCheck`**:
  - Return `HealthCheckResponse { status: "ok".to_string() }`
  - Optionally verify Qdrant and Ollama connectivity

- [ ] 7.9. **Stub remaining methods** — Return `tonic::Status::unimplemented("Not supported")` for: `GetWorkspaceInfo`, `ListWorkspaces`, `DeleteWorkspace`, `ChunkFiles`, `ValidateFiles`, `SelectSkill`, `FuzzySearch`

### Phase 8: Testing & Integration

- [ ] 8.1. **Unit tests for `chunker.rs`**:
  - Empty file → 0 chunks
  - Small file (< min_size) → 1 chunk
  - Large file → multiple chunks with correct line numbers
  - Line boundaries are respected (no mid-line splits)

- [ ] 8.2. **Unit tests for `auth.rs`**:
  - Valid token returns user_id
  - Missing header returns unauthenticated
  - Invalid token returns unauthenticated

- [ ] 8.3. **Integration test: full sync cycle**:
  - Call `CreateApiKey` → get token
  - Call `CreateWorkspace` → get workspace_id
  - Call `UploadFiles` with test files
  - Call `ListFiles` → verify file hashes match SHA-256 of uploaded content
  - Call `Search` → verify results contain uploaded content
  - Call `DeleteFiles` → verify files removed
  - Call `ListFiles` → verify empty

- [ ] 8.4. **Hash compatibility test**:
  - Verify that the server's SHA-256 hash output matches the Forge client's `compute_hash` function (hex-encoded SHA-256 of raw content string). Use the same test vectors:
    ```
    compute_hash("old content") == sha256_hex("old content")
    compute_hash("new content") == sha256_hex("new content")
    ```

### Phase 9: Docker & Deployment

- [ ] 9.1. **Create `docker-compose.yml`** with three services:
  ```yaml
  services:
    workspace-server:
      build: .
      ports: ["50051:50051"]
      environment:
        QDRANT_URL: http://qdrant:6334
        OLLAMA_URL: http://ollama:11434
      depends_on: [qdrant, ollama]
    
    qdrant:
      image: qdrant/qdrant:latest
      ports: ["6333:6333", "6334:6334"]
      volumes: ["qdrant_data:/qdrant/storage"]
    
    ollama:
      image: ollama/ollama:latest
      ports: ["11434:11434"]
      volumes: ["ollama_data:/root/.ollama"]
  ```

- [ ] 9.2. **Create `Dockerfile`** — multi-stage build:
  - Stage 1: Build with `rust:1.79-bookworm` + `protobuf-compiler`
  - Stage 2: Runtime with `debian:bookworm-slim`

- [ ] 9.3. **Create startup script** that pulls `nomic-embed-text` model on first Ollama boot:
  ```bash
  ollama pull nomic-embed-text
  ```

---

## Verification Criteria

1. **Forge CLI connects successfully** — `FORGE_WORKSPACE_SERVER_URL=http://localhost:50051` allows forge to complete `:sync` and `:workspace-init` without errors
2. **Incremental sync works** — second `:sync` call only uploads changed files (verified by `ListFiles` hash comparison)
3. **Semantic search returns relevant results** — `sem_search` tool returns `FileChunk` nodes with correct `file_path`, `content`, `start_line`, `end_line`
4. **Hash compatibility** — `ListFiles` returns hashes identical to what the Forge client computes locally via `sha2::Sha256`
5. **Concurrent uploads** — server handles parallel `UploadFiles` calls without data corruption
6. **Fully offline** — no external network calls (Qdrant and Ollama run locally)

---

## Potential Risks and Mitigations

1. **Qdrant collection naming collisions**
   Collections named `ws_{uuid}` are globally unique per workspace. Risk is negligible since workspace IDs are UUID v4.
   Mitigation: None needed.

2. **Ollama embedding latency during large syncs**
   `nomic-embed-text` is fast (~1ms per embedding on CPU) but a large codebase (10k+ files) will generate many chunks.
   Mitigation: Batch embedding calls (Ollama `/api/embed` supports arrays). Process files sequentially but chunks within a file in batch. The client already serializes uploads per-file, so server-side batching within a single `UploadFiles` call is sufficient.

3. **SQLite write contention under concurrent uploads**
   Concurrent `UploadFiles` calls all write to `file_refs` table.
   Mitigation: Use `rusqlite` with WAL mode (`PRAGMA journal_mode=WAL`) which allows concurrent reads and serialized writes without blocking. Wrap `Connection` in `Arc<Mutex<Connection>>` for thread safety.

4. **File re-upload leaves stale chunks in Qdrant**
   If a file is modified and re-uploaded, old chunks must be deleted first.
   Mitigation: Step 7.4 explicitly deletes all existing chunks for a file path before inserting new ones. This is done within the same `UploadFiles` handler.

5. **Proto compatibility drift**
   If the upstream `forge.proto` changes, the server must be updated.
   Mitigation: Pin to a specific proto version. The proto is stable (all MVP methods already exist and the client's usage patterns are well-understood from `context_engine.rs`).

6. **No reranking in MVP**
   The production server likely uses a cross-encoder for reranking (the `relevance_query` field). The MVP uses raw embedding similarity as relevance.
   Mitigation: Acceptable for MVP. Results will be less precise but functional. A reranking pass with a local model (e.g., `BAAI/bge-reranker-base` via Ollama or a small ONNX model) can be added in a follow-up.

7. **Embedding model dimension mismatch**
   If someone uses a different Ollama model, the vector dimension won't be 768.
   Mitigation: Make `embedding_dim` configurable. Validate dimension on first embedding call and fail fast with a clear error.

---

## Alternative Approaches

1. **In-memory metadata instead of SQLite**: Store api_keys and workspaces in `HashMap` behind `Arc<RwLock<>>`. Simpler but data is lost on restart, requiring full re-sync every time the server restarts. **Not recommended** for any real usage.

2. **PostgreSQL + pgvector instead of Qdrant**: Single database for both metadata and vectors. Reduces infrastructure but pgvector ANN performance is significantly worse than Qdrant for large collections. **Not recommended** for MVP.

3. **Streaming UploadFiles**: Use gRPC server streaming to accept large file batches. The current client sends files one-at-a-time (unary calls), so streaming provides no benefit. **Not needed**.

4. **Tree-sitter based chunking**: Use language-aware AST parsing to split files at function/class boundaries instead of line-count based splitting. Better semantic coherence but requires tree-sitter grammar for every language. **Good follow-up enhancement** after MVP.

5. **Pre-built embedding server (text-embeddings-inference)**: Use Hugging Face TEI instead of Ollama for embeddings. Better batching and GPU support but adds another Docker image and doesn't integrate as cleanly with the Ollama ecosystem. **Consider if performance is insufficient with Ollama**.
