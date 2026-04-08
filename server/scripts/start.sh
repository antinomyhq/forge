#!/usr/bin/env bash
set -euo pipefail

# Forge Workspace Server — setup & launch
# Usage: ./server/scripts/start.sh [--ollama-url URL] [--stop] [--status]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVER_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_DIR="$(cd "$SERVER_DIR/.." && pwd)"

# Defaults
QDRANT_CONTAINER="forge-qdrant"
QDRANT_PORT_HTTP=6333
QDRANT_PORT_GRPC=6334
OLLAMA_URL="${OLLAMA_URL:-http://localhost:11434}"
SERVER_PID_FILE="$SERVER_DIR/.server.pid"
SERVER_LOG_FILE="$SERVER_DIR/server.log"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------
ACTION="start"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --ollama-url)   OLLAMA_URL="$2"; shift 2 ;;
        --stop)         ACTION="stop"; shift ;;
        --status)       ACTION="status"; shift ;;
        --help|-h)      ACTION="help"; shift ;;
        *)              error "Unknown argument: $1"; exit 1 ;;
    esac
done

# ---------------------------------------------------------------------------
# Help
# ---------------------------------------------------------------------------
if [[ "$ACTION" == "help" ]]; then
    cat <<EOF
Forge Workspace Server — setup & launch

Usage:
  ./server/scripts/start.sh                     Start everything (Qdrant + Server)
  ./server/scripts/start.sh --ollama-url URL    Use custom Ollama endpoint
  ./server/scripts/start.sh --stop              Stop server and Qdrant
  ./server/scripts/start.sh --status            Check status of all services

Environment variables:
  OLLAMA_URL        Ollama endpoint (default: http://localhost:11434)
  QDRANT_URL        Qdrant gRPC endpoint (default: http://localhost:6334)
  LISTEN_ADDR       Server listen address (default: 0.0.0.0:50051)
  EMBEDDING_MODEL   Ollama model (default: nomic-embed-text)
  DB_PATH           SQLite database path (default: ./forge-server.db)
  RUST_LOG          Log level (default: info)
EOF
    exit 0
fi

# ---------------------------------------------------------------------------
# Stop
# ---------------------------------------------------------------------------
if [[ "$ACTION" == "stop" ]]; then
    info "Stopping Forge Workspace Server..."
    if [[ -f "$SERVER_PID_FILE" ]]; then
        PID=$(cat "$SERVER_PID_FILE")
        if kill -0 "$PID" 2>/dev/null; then
            kill "$PID"
            info "Server stopped (PID $PID)"
        else
            warn "Server process $PID not running"
        fi
        rm -f "$SERVER_PID_FILE"
    else
        warn "No PID file found"
    fi

    if docker ps -q -f name="$QDRANT_CONTAINER" 2>/dev/null | grep -q .; then
        docker stop "$QDRANT_CONTAINER" >/dev/null 2>&1
        info "Qdrant stopped"
    else
        warn "Qdrant container not running"
    fi
    exit 0
fi

# ---------------------------------------------------------------------------
# Status
# ---------------------------------------------------------------------------
if [[ "$ACTION" == "status" ]]; then
    echo ""
    echo "=== Forge Workspace Server Status ==="
    echo ""

    # Qdrant
    if docker ps -q -f name="$QDRANT_CONTAINER" 2>/dev/null | grep -q .; then
        echo -e "  Qdrant:    ${GREEN}running${NC}  (localhost:$QDRANT_PORT_HTTP)"
    else
        echo -e "  Qdrant:    ${RED}stopped${NC}"
    fi

    # Ollama
    if curl -sf "$OLLAMA_URL" >/dev/null 2>&1; then
        echo -e "  Ollama:    ${GREEN}running${NC}  ($OLLAMA_URL)"
    else
        echo -e "  Ollama:    ${RED}unreachable${NC}  ($OLLAMA_URL)"
    fi

    # Server
    if [[ -f "$SERVER_PID_FILE" ]] && kill -0 "$(cat "$SERVER_PID_FILE")" 2>/dev/null; then
        echo -e "  Server:    ${GREEN}running${NC}  (PID $(cat "$SERVER_PID_FILE"), localhost:50051)"
    else
        echo -e "  Server:    ${RED}stopped${NC}"
    fi

    echo ""
    exit 0
fi

# ---------------------------------------------------------------------------
# Start
# ---------------------------------------------------------------------------
info "Starting Forge Workspace Server..."
echo ""

# 1. Check prerequisites
command -v docker >/dev/null 2>&1 || { error "Docker is required but not installed"; exit 1; }
command -v cargo  >/dev/null 2>&1 || { error "Rust/Cargo is required but not installed"; exit 1; }
command -v curl   >/dev/null 2>&1 || { error "curl is required but not installed"; exit 1; }

# 2. Start Qdrant
if docker ps -q -f name="$QDRANT_CONTAINER" 2>/dev/null | grep -q .; then
    info "Qdrant already running"
else
    if docker ps -aq -f name="$QDRANT_CONTAINER" 2>/dev/null | grep -q .; then
        info "Starting existing Qdrant container..."
        docker start "$QDRANT_CONTAINER" >/dev/null
    else
        info "Creating and starting Qdrant container..."
        docker run -d \
            --name "$QDRANT_CONTAINER" \
            -p "$QDRANT_PORT_HTTP:6333" \
            -p "$QDRANT_PORT_GRPC:6334" \
            -v forge_qdrant_data:/qdrant/storage \
            qdrant/qdrant:latest >/dev/null
    fi

    # Wait for Qdrant to be ready
    info "Waiting for Qdrant..."
    for i in $(seq 1 30); do
        if curl -sf "http://localhost:$QDRANT_PORT_HTTP/readyz" >/dev/null 2>&1; then
            break
        fi
        sleep 1
    done

    if ! curl -sf "http://localhost:$QDRANT_PORT_HTTP/readyz" >/dev/null 2>&1; then
        error "Qdrant failed to start within 30 seconds"
        exit 1
    fi
    info "Qdrant ready"
fi

# 3. Check Ollama
info "Checking Ollama at $OLLAMA_URL..."
if ! curl -sf "$OLLAMA_URL" >/dev/null 2>&1; then
    error "Ollama is not reachable at $OLLAMA_URL"
    error "Either start Ollama or set --ollama-url to the correct address"
    exit 1
fi

# Check if model is available
if ! curl -sf "$OLLAMA_URL/api/tags" 2>/dev/null | grep -q "nomic-embed-text"; then
    warn "Model 'nomic-embed-text' not found. Pulling..."
    curl -sf "$OLLAMA_URL/api/pull" -d '{"name":"nomic-embed-text"}' >/dev/null 2>&1 || true
    info "Model pull initiated. This may take a few minutes on first run."
fi
info "Ollama ready"

# 4. Stop old server if running
if [[ -f "$SERVER_PID_FILE" ]]; then
    OLD_PID=$(cat "$SERVER_PID_FILE")
    if kill -0 "$OLD_PID" 2>/dev/null; then
        info "Stopping old server (PID $OLD_PID)..."
        kill "$OLD_PID" 2>/dev/null || true
        sleep 1
    fi
    rm -f "$SERVER_PID_FILE"
fi

# Also kill any process occupying port 50051
EXISTING_PIDS=$(lsof -ti :50051 2>/dev/null || true)
if [[ -n "$EXISTING_PIDS" ]]; then
    warn "Port 50051 is occupied. Killing existing processes..."
    echo "$EXISTING_PIDS" | xargs kill -9 2>/dev/null || true
    sleep 1
fi

# 5. Build server
info "Building server..."
cargo build --manifest-path "$SERVER_DIR/Cargo.toml" 2>&1 | tail -3

# 6. Start server
info "Starting gRPC server on 0.0.0.0:50051..."
OLLAMA_URL="$OLLAMA_URL" RUST_LOG="${RUST_LOG:-info}" \
    nohup "$SERVER_DIR/target/debug/forge-workspace-server" > "$SERVER_LOG_FILE" 2>&1 &
echo $! > "$SERVER_PID_FILE"

# Wait for server to be ready
sleep 2
if kill -0 "$(cat "$SERVER_PID_FILE")" 2>/dev/null; then
    info "Server started (PID $(cat "$SERVER_PID_FILE"))"
else
    error "Server failed to start. Check logs: $SERVER_LOG_FILE"
    cat "$SERVER_LOG_FILE"
    exit 1
fi

echo ""
echo "=========================================="
echo "  Forge Workspace Server is running!"
echo "=========================================="
echo ""
echo "  gRPC endpoint:  localhost:50051"
echo "  Qdrant:          localhost:$QDRANT_PORT_HTTP"
echo "  Ollama:          $OLLAMA_URL"
echo "  Logs:            $SERVER_LOG_FILE"
echo "  PID file:        $SERVER_PID_FILE"
echo ""
echo "  To connect Forge CLI:"
echo "    export FORGE_SERVICES_URL=http://localhost:50051"
echo "    # or add to ~/.env:"
echo "    # FORGE_SERVICES_URL=http://localhost:50051"
echo ""
echo "  To stop:"
echo "    ./server/scripts/start.sh --stop"
echo ""
echo "  To check status:"
echo "    ./server/scripts/start.sh --status"
echo ""
