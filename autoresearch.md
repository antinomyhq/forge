# Autoresearch: reduce Forge workspace-service memory

## Objective
Reduce peak memory usage in Forge's workspace indexing/status services. The current code path reads and retains full file contents for every tracked source file during sync/status, even when remote hashes are already in sync. The goal is to cut peak RSS by avoiding unnecessary full-file retention while preserving behavior.

## Metrics
- **Primary**: peak_rss_mb (MB, lower is better)
- **Secondary**: benchmark wall time, files processed, uploaded_files, failed_files

## How to Run
`./autoresearch.sh`

The script builds and runs `crates/forge_services/examples/workspace_sync_memory.rs`, which:
- creates a synthetic git workspace with many tracked `.rs` files
- configures a mock remote workspace whose hashes already match local files
- runs `ForgeWorkspaceService::sync_workspace()` or `get_workspace_status()`
- prints a success marker consumed by `autoresearch.sh`

## Files in Scope
- `crates/forge_services/src/context_engine.rs` — workspace sync/status implementation
- `crates/forge_app/src/workspace_status.rs` — sync planning utilities
- `crates/forge_services/examples/workspace_sync_memory.rs` — synthetic memory benchmark harness
- `autoresearch.sh` — benchmark runner and metric extraction
- `autoresearch.md` — session context

## Off Limits
- Protocol/schema changes for the remote workspace server
- Unrelated UI/CLI behavior
- New third-party dependencies unless absolutely necessary

## Constraints
- Keep sync/status behavior correct
- Existing tests must pass
- No release builds unless absolutely necessary
- Favor simpler data-flow changes over broad architectural rewrites

## What's Been Tried
- Added a synthetic workspace-sync memory benchmark that exercises the current service path on a large tracked repo with in-sync remote hashes.
- Initial hypothesis: peak memory is dominated by collecting `Vec<FileNode>` for all files and cloning/hash-planning from that full-content buffer.
