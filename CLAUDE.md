# CLAUDE.md

## Testing

Use `cargo nextest run` instead of `cargo test`. The project is configured for nextest (see `.config/nextest.toml`).

Always pass `--no-input-handler` to avoid a crossterm panic in non-interactive environments (e.g. when run by an LLM agent).

```bash
# Only unit tests (fast feedback loop during development)
cargo nextest run --no-input-handler --lib

# Specific crate
cargo nextest run --no-input-handler -p forge_domain

# Integration tests only
cargo nextest run --no-input-handler --test '*'

# Watch mode (auto-rerun on file changes)
cargo watch -x "nextest run --no-input-handler --lib"
```

### Final verification

Before considering any task complete, run the **full** workspace test suite **once** at the very end. This is the same command CI uses and catches issues that crate-scoped runs miss (feature-flag interactions, integration tests, cross-crate breakage):

```bash
cargo nextest run --no-input-handler --all-features --workspace
```

Do NOT run this command repeatedly during development — use the crate-scoped commands above for iteration. Run it exactly once as the last step.

Do NOT silently skip work. If a task is out of scope for the current change, place the TODO and mention it in your response summary.
