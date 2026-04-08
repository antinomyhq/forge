# README: Add Self-Hosted Workspace Server Section

## Objective

Add a comprehensive section to `README.md` describing the self-hosted workspace server (`server/`), its architecture, setup, and usage.

## Implementation Plan

- [ ] 1. Add ToC entry — insert after `- [Documentation](#documentation)` line 45, before `- [Community](#community)` line 46:
  ```
  - [Self-Hosted Workspace Server](#self-hosted-workspace-server)
    - [Architecture](#architecture)
    - [Prerequisites](#prerequisites)
    - [Quick Start](#quick-start)
    - [Server Configuration](#server-configuration)
    - [Connecting Forge to the Server](#connecting-forge-to-the-server)
    - [How It Works](#how-it-works)
    - [Docker Deployment](#docker-deployment)
  ```

- [ ] 2. Insert server section — after line 1093 (`---` after Documentation section), before line 1095 (`## Installation`). The full content is below.

## Full Section Content

```markdown
## Self-Hosted Workspace Server

The `server/` directory contains a self-hosted gRPC server that powers Forge's semantic search and workspace indexing. Instead of sending your code to an external service, everything runs locally — your code never leaves your machine.

### Architecture

```
┌─────────────┐      gRPC       ┌──────────────────┐
│  Forge CLI   │ ◄────────────► │  Workspace Server │
│  (:sync,     │   port 50051   │  (Rust / tonic)   │
│   :search)   │                └────────┬──────────┘
└─────────────┘                          │
                              ┌──────────┼──────────┐
                              ▼          ▼          ▼
                         ┌────────┐ ┌────────┐ ┌────────┐
                         │ SQLite │ │ Qdrant │ │ Ollama │
                         │metadata│ │vectors │ │embeddings│
                         └────────┘ └────────┘ └────────┘
```

| Component | Role | Storage |
|-----------|------|---------|
| **Workspace Server** | gRPC API, file chunking, orchestration | — |
| **SQLite** | API keys, workspaces, file references | `./forge-server.db` |
| **Qdrant** | Vector storage and ANN search | Docker volume |
| **Ollama** | Local text embeddings (`nomic-embed-text`, 768-dim) | Model cache |

### Prerequisites

- **Rust toolchain** (1.85+) — for building the server
- **protobuf compiler** — `brew install protobuf` (macOS) or `apt install protobuf-compiler` (Linux)
- **Docker** — for running Qdrant
- **Ollama** — running locally or on your network, with `nomic-embed-text` model pulled

### Quick Start

```bash
# 1. Start Qdrant
docker run -d --name forge-qdrant \
  -p 6333:6333 -p 6334:6334 \
  -v forge_qdrant_data:/qdrant/storage \
  qdrant/qdrant:latest

# 2. Ensure Ollama has the embedding model
ollama pull nomic-embed-text

# 3. Build and run the server
cd server
cargo build --release
./target/release/forge-workspace-server

# 4. In another terminal, use Forge as usual
forge
# Then run :sync to index your codebase
```

### Server Configuration

All settings can be set via CLI flags or environment variables:

| Environment Variable | CLI Flag | Default | Description |
|---------------------|----------|---------|-------------|
| `LISTEN_ADDR` | `--listen-addr` | `0.0.0.0:50051` | gRPC listen address |
| `QDRANT_URL` | `--qdrant-url` | `http://localhost:6334` | Qdrant gRPC endpoint |
| `OLLAMA_URL` | `--ollama-url` | `http://localhost:11434` | Ollama HTTP endpoint |
| `EMBEDDING_MODEL` | `--embedding-model` | `nomic-embed-text` | Ollama model name |
| `EMBEDDING_DIM` | `--embedding-dim` | `768` | Vector dimension |
| `DB_PATH` | `--db-path` | `./forge-server.db` | SQLite database path |
| `CHUNK_MAX_SIZE` | `--chunk-max-size` | `1500` | Max chunk size (bytes) |
| `CHUNK_MIN_SIZE` | `--chunk-min-size` | `100` | Min chunk size (bytes) |

Example with custom Ollama on a network host:

```bash
OLLAMA_URL=http://192.168.1.100:11434 ./target/release/forge-workspace-server
```

### Connecting Forge to the Server

Forge reads the server URL from configuration. The default is already `http://localhost:50051`, so if you're running the server locally, no extra configuration is needed.

To point Forge to a different server:

```bash
# Option 1: Environment variable (in .env or shell)
export FORGE_SERVICES_URL=http://your-server:50051

# Option 2: forge.toml
# In ~/forge/.forge.toml or .forge.toml in your project:
services_url = "http://your-server:50051"
```

### How It Works

**Indexing (`:sync`)**

1. Forge reads all project files and computes SHA-256 hashes
2. Compares hashes with the server via `ListFiles` — only changed files are uploaded
3. Each file is split into line-aware chunks (respecting `max_chunk_size`)
4. Chunks are embedded via Ollama (`nomic-embed-text`, 768-dimensional vectors)
5. Vectors + metadata are stored in Qdrant

**Searching**

1. Your query is embedded into a vector via Ollama
2. Qdrant performs approximate nearest neighbor (ANN) search
3. The top matching code chunks are returned to Forge
4. Forge includes only these relevant chunks in the LLM context — not your entire codebase

This reduces token usage by 5-10x per request while improving answer quality, since the LLM sees exactly the relevant code.

### Docker Deployment

For a fully containerized setup, use the included `docker-compose.yml`:

```bash
cd server

# Start all services (server + Qdrant + Ollama)
docker compose up -d

# Pull the embedding model into the Ollama container
docker compose exec ollama ollama pull nomic-embed-text

# Verify
grpcurl -plaintext localhost:50051 forge.v1.ForgeService/HealthCheck
```

The compose file starts three services:

| Service | Port | Volume |
|---------|------|--------|
| `workspace-server` | `50051` | `server_data:/data` |
| `qdrant` | `6333` (HTTP), `6334` (gRPC) | `qdrant_data` |
| `ollama` | `11434` | `ollama_data` |
```

## Verification Criteria

- Section appears in the Table of Contents with correct anchor links
- Section is placed between "Documentation" and "Installation" (or between "Installation" and "Community")
- Architecture diagram renders correctly in GitHub markdown
- All env variables and CLI flags match `server/src/config.rs`
- Quick start steps are verified to work
