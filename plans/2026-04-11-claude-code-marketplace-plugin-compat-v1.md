# Claude Code Marketplace Plugin Compatibility

## Objective

Enable Forge to fully discover, install, and activate plugins authored for Claude Code, including marketplace-structured plugins. Currently, when a Claude Code marketplace plugin (e.g., `claude-mem`) is installed via `forge plugin install`, all components show 0/none because Forge doesn't understand the marketplace directory indirection (`marketplace.json` → `source: "./plugin"`) and doesn't expose `CLAUDE_PLUGIN_ROOT` for hook/MCP subprocess variable substitution.

**Expected outcome**: After these changes, `forge plugin install` of a Claude Code marketplace plugin correctly counts and displays all components (skills, commands, agents, hooks, MCP servers), and enabling the plugin makes its hooks and MCP servers fully operational.

## Context

### Claude Code Marketplace Plugin Structure

A marketplace repository has this layout:

```
thedotmack/                              <-- repo root (passed to `forge plugin install`)
├── .claude-plugin/
│   ├── plugin.json                      <-- repo-level manifest (name, version, author)
│   └── marketplace.json                 <-- marketplace indirection: {"plugins": [{"source": "./plugin"}]}
├── .mcp.json                            <-- EMPTY: {"mcpServers": {}}
├── plugin/                              <-- REAL plugin root
│   ├── .claude-plugin/plugin.json       <-- plugin-level manifest
│   ├── .mcp.json                        <-- 1 MCP server (mcp-search)
│   ├── hooks/hooks.json                 <-- 7 hook events
│   ├── skills/                          <-- 7 skills (subdirs with SKILL.md)
│   ├── scripts/                         <-- executables (mcp-server.cjs, etc.)
│   └── modes/                           <-- Claude Code-specific modes
└── src/, tests/, ...                    <-- repo dev files (not part of plugin)
```

### Current Forge Behavior

1. **`scan_root`** (`crates/forge_repo/src/plugin.rs:161-212`): Scans one level deep only. For `~/.claude/plugins/`, it finds `marketplaces/` as a child dir, but `marketplaces/` has no manifest → silently skipped.
2. **`find_install_manifest`** (`crates/forge_main/src/ui.rs:167-187`): Finds `.claude-plugin/plugin.json` at the repo root (not the real plugin at `./plugin/`).
3. **`count_entries`** (`crates/forge_main/src/ui.rs:271-276`): Counts `skills/`, `commands/`, `agents/` at the repo root → 0 because they don't exist there.
4. **MCP count** (`ui.rs:4864`): Only checks `manifest.mcp_servers` (not `.mcp.json` sidecar) → 0.
5. **Hook env vars** (`crates/forge_app/src/hooks/plugin.rs:269-271`): Only injects `FORGE_PLUGIN_ROOT`, not `CLAUDE_PLUGIN_ROOT` → Claude Code hooks using `${CLAUDE_PLUGIN_ROOT}` fail.
6. **MCP env vars** (`crates/forge_services/src/mcp/manager.rs:117`): Same — only `FORGE_PLUGIN_ROOT`.

## Implementation Plan

### Phase 1: Marketplace Directory Support for Runtime Plugin Discovery

This phase makes `scan_root` able to discover plugins inside Claude Code's `marketplaces/` and `cache/` subdirectory structures, which use `marketplace.json` for indirection.

- [x] **Task 1.1. Add `marketplace.json` deserialization type to `forge_domain`**

  **File:** `crates/forge_domain/src/plugin.rs`

  Add a new struct `MarketplaceManifest` with the shape:
  ```
  { "plugins": [{ "name": "...", "source": "./plugin", ... }] }
  ```
  The key field is `source` (relative path from the marketplace.json to the real plugin root). Use `#[serde(rename_all = "camelCase")]` for Claude Code wire compat. The `plugins` field is a `Vec<MarketplacePluginEntry>` where each entry has at minimum `name: Option<String>` and `source: String`.

  **Rationale:** Forge needs a way to parse this indirection file to resolve the actual plugin root within marketplace directories.

- [x] **Task 1.2. Add marketplace-aware scanning to `scan_root`**

  **File:** `crates/forge_repo/src/plugin.rs` (function `scan_root` at lines 161-212)

  Currently `scan_root` iterates immediate child directories and calls `load_one_plugin` on each. The change:
  - After `load_one_plugin` returns `Ok(None)` (no manifest found), check for `<child>/.claude-plugin/marketplace.json` or `<child>/marketplace.json`.
  - If found, parse it as `MarketplaceManifest`.
  - For each entry in `plugins`, resolve `<child>/<entry.source>` as a new plugin directory and call `load_one_plugin` on it.
  - This adds a second scan level specifically for marketplace indirection without general recursive descent.

  **Rationale:** Claude Code stores marketplace plugins at `~/.claude/plugins/marketplaces/<author>/` with `marketplace.json` pointing to nested plugin directories. Without this, marketplace plugins are invisible to Forge.

- [x] **Task 1.3. Handle `cache/` versioned directory layout**

  **File:** `crates/forge_repo/src/plugin.rs`

  Claude Code also uses `~/.claude/plugins/cache/<author>/<plugin>/<version>/` layout. The `hooks.json` in claude-mem references this path pattern. Add handling in `scan_root`:
  - When scanning `~/.claude/plugins/` and encountering a `cache/` or `marketplaces/` child directory, scan two levels deeper (author → plugin-or-version) looking for manifests.
  - Alternatively, detect these known directory names and apply marketplace.json-based resolution.

  **Rationale:** Some Claude Code plugins are installed via npm/cache mechanisms that create versioned directory hierarchies. Forge should discover both layouts.

- [x] **Task 1.4. Add test fixture for marketplace plugin structure**

  **Files:**
  - `crates/forge_repo/src/fixtures/plugins/marketplace_plugin/` (new directory)
  - Or `crates/forge_services/tests/fixtures/plugins/marketplace-provider/`

  Create a minimal fixture replicating the marketplace layout:
  ```
  marketplace-provider/
  ├── .claude-plugin/
  │   ├── plugin.json       (repo-level manifest)
  │   └── marketplace.json  (source: "./plugin")
  ├── plugin/
  │   ├── .claude-plugin/plugin.json
  │   ├── .mcp.json
  │   ├── hooks/hooks.json
  │   ├── skills/demo-skill/SKILL.md
  │   └── commands/demo-cmd.md
  ```

  Add tests in `crates/forge_repo/src/plugin.rs` (inline test module) verifying:
  - `scan_root` discovers the nested plugin (not the repo root).
  - Component paths (skills, commands) resolve correctly.
  - MCP servers from the nested `.mcp.json` are picked up.
  - The repo-root `.claude-plugin/plugin.json` is NOT loaded as a separate plugin.

  **Rationale:** Without fixture tests, regressions in marketplace discovery will go undetected.

### Phase 2: Install-Time Marketplace Awareness

This phase makes `forge plugin install <path>` correctly handle marketplace directories by locating the real plugin root and counting its components.

- [x] **Task 2.1. Detect marketplace indirection during install**

  **File:** `crates/forge_main/src/ui.rs` (function `on_plugin_install` at lines 4791-4930)

  After finding and parsing the manifest (step 2, lines 4804-4824), add marketplace resolution:
  - Check for a sibling `marketplace.json` next to the found `plugin.json`.
  - If present, parse it as `MarketplaceManifest`.
  - If there's exactly one plugin entry with a `source` field, resolve `<source_dir>/<source>` as the effective plugin root.
  - Re-locate and re-parse the manifest from the effective root.
  - Use the effective root for component counting (step 4) and file copying (step 5).

  **Rationale:** When a user runs `forge plugin install /path/to/marketplace/author`, we should install the actual plugin (e.g., `./plugin/`), not the entire marketplace repo.

- [x] **Task 2.2. Count MCP servers from `.mcp.json` sidecar in trust prompt**

  **File:** `crates/forge_main/src/ui.rs` (around line 4864)

  Currently:
  ```rust
  let mcp_count = manifest.mcp_servers.as_ref().map(|m| m.len()).unwrap_or(0);
  ```

  Change to also parse the `.mcp.json` sidecar file at `<source>/.mcp.json`, merging counts the same way `resolve_mcp_servers` does in `crates/forge_repo/src/plugin.rs:353-401`. Extract the parsing logic into a shared helper or duplicate the minimal parse-and-count logic inline.

  **Rationale:** Claude Code plugins typically declare MCP servers in `.mcp.json`, not in the manifest. Without this, the trust prompt always shows "MCP Servers: 0" for Claude Code plugins.

- [x] **Task 2.3. Copy only the effective plugin root (not the entire marketplace repo)**

  **File:** `crates/forge_main/src/ui.rs` (step 5, lines 4900-4915)

  When marketplace indirection was detected in Task 2.1, `copy_dir_recursive` should copy from the effective plugin root (e.g., `<source>/plugin/`) to the target, not from the original `<source>` (the entire marketplace repo with `src/`, `tests/`, `node_modules/`, etc.).

  **Rationale:** Copying the entire marketplace repo wastes disk space and includes dev files, tests, and other non-plugin content. Claude Code's own installer only copies the plugin subdirectory.

- [x] **Task 2.4. Add install-time tests for marketplace plugins**

  **File:** `crates/forge_main/src/ui.rs` or a new integration test file

  Test that:
  - `find_install_manifest` + marketplace detection resolves to the nested plugin.
  - Component counts reflect the nested plugin's actual content.
  - `copy_dir_recursive` copies from the effective root.

  **Rationale:** Ensures the install flow works end-to-end for marketplace-structured plugins.

### Phase 3: `CLAUDE_PLUGIN_ROOT` Environment Variable Alias

This phase ensures Claude Code plugin hooks and MCP servers that reference `${CLAUDE_PLUGIN_ROOT}` work under Forge without any plugin-side modifications.

- [x] **Task 3.1. Add `CLAUDE_PLUGIN_ROOT` alias for hook subprocesses**

  **File:** `crates/forge_app/src/hooks/plugin.rs` (around lines 269-272)

  After inserting `FORGE_PLUGIN_ROOT`, also insert `CLAUDE_PLUGIN_ROOT` with the same value:
  ```rust
  if let Some(ref root) = source.plugin_root {
      let root_str = root.display().to_string();
      env_vars.insert(FORGE_PLUGIN_ROOT.to_string(), root_str.clone());
      env_vars.insert("CLAUDE_PLUGIN_ROOT".to_string(), root_str);
  }
  ```

  Also add the constant `const CLAUDE_PLUGIN_ROOT: &str = "CLAUDE_PLUGIN_ROOT";` alongside the existing constants (line 46).

  **Rationale:** Claude Code plugins universally use `${CLAUDE_PLUGIN_ROOT}` in hook commands. The `substitute_variables` function (`crates/forge_services/src/hook_runtime/shell.rs:483-516`) replaces `${VAR}` from the env_vars map, and the shell itself expands `$CLAUDE_PLUGIN_ROOT`. Both paths require the variable to be present in the env map.

- [x] **Task 3.2. Add `CLAUDE_PLUGIN_ROOT` alias for MCP server subprocesses**

  **File:** `crates/forge_services/src/mcp/manager.rs` (around line 117)

  After injecting `FORGE_PLUGIN_ROOT` into stdio server env, also inject `CLAUDE_PLUGIN_ROOT`:
  ```rust
  stdio.env
      .entry(FORGE_PLUGIN_ROOT_ENV.to_string())
      .or_insert_with(|| plugin_root.clone());
  stdio.env
      .entry("CLAUDE_PLUGIN_ROOT".to_string())
      .or_insert_with(|| plugin_root.clone());
  ```

  **Rationale:** MCP server commands from Claude Code plugins also reference `${CLAUDE_PLUGIN_ROOT}` (e.g., `"command": "${CLAUDE_PLUGIN_ROOT}/scripts/mcp-server.cjs"`). The same variable needs to be available in the MCP subprocess environment.

- [x] **Task 3.3. Update reference env builder and tests**

  **Files:**
  - `crates/forge_services/src/hook_runtime/env.rs` (reference `build_hook_env_vars` function)
  - `crates/forge_app/src/hooks/plugin.rs` (existing tests)
  - `crates/forge_services/src/mcp/manager.rs` (existing tests)

  Update the reference builder to also produce `CLAUDE_PLUGIN_ROOT` when `plugin_root` is provided. Update all existing tests that assert on env var maps to expect the new alias. Add a specific test verifying that `${CLAUDE_PLUGIN_ROOT}` in a command string is correctly substituted.

  **Rationale:** Test coverage ensures the alias doesn't regress and that both `${FORGE_PLUGIN_ROOT}` and `${CLAUDE_PLUGIN_ROOT}` work in command strings.

### Phase 4: `CLAUDE_PROJECT_DIR` and `CLAUDE_SESSION_ID` Aliases

Similar to Phase 3, Claude Code hooks may also reference `CLAUDE_PROJECT_DIR` and other `CLAUDE_*` prefixed variables.

- [x] **Task 4.1. Add remaining `CLAUDE_*` env var aliases for hooks**

  **File:** `crates/forge_app/src/hooks/plugin.rs` (env var construction block, lines 262-307)

  Add aliases:
  - `CLAUDE_PROJECT_DIR` → same as `FORGE_PROJECT_DIR`
  - `CLAUDE_SESSION_ID` → same as `FORGE_SESSION_ID`

  Only add these when the hook source is a `ClaudeCode` plugin (check `source.source == PluginSource::ClaudeCode`) to avoid polluting the env for Forge-native plugins.

  **Rationale:** Some Claude Code hooks reference `$CLAUDE_PROJECT_DIR`. Conditional injection based on plugin source avoids adding unnecessary variables for Forge-native plugins.

- [x] **Task 4.2. Add `CLAUDE_PROJECT_DIR` alias for MCP subprocesses**

  **File:** `crates/forge_services/src/mcp/manager.rs`

  Alongside `FORGE_PROJECT_DIR`, also inject `CLAUDE_PROJECT_DIR` with the same value for plugin-contributed MCP servers.

  **Rationale:** MCP server scripts may use `$CLAUDE_PROJECT_DIR` in their runtime logic.

- [x] **Task 4.3. Update tests for all aliases**

  **Files:** Same as Task 3.3 plus any new tests needed for `CLAUDE_PROJECT_DIR` / `CLAUDE_SESSION_ID`.

  **Rationale:** Ensures complete test coverage for all Claude Code env var aliases.

### Phase 5: Trust Prompt Modes Component Display (Optional Enhancement)

Claude Code plugins may include `modes/` directories with custom operational modes. This is a lower-priority enhancement for display completeness.

- [x] **Task 5.1. Add `modes` count to trust prompt COMPONENTS section**

  **File:** `crates/forge_main/src/ui.rs` (trust prompt section, around line 4878)

  Add a line:
  ```rust
  let modes_count = count_entries(&source, "modes");
  ```
  And display it in the COMPONENTS section if > 0.

  **Rationale:** Gives users visibility into plugin modes during the trust prompt. Modes are informational only — Forge doesn't execute them — but seeing "Modes: 36" helps users understand what the plugin contains.

- [x] **Task 5.2. Add `modes` count to `/plugin info` and `/plugin list`**

  **Files:** `crates/forge_main/src/ui.rs` (functions `on_plugin_info` at line 4691, `format_plugin_components` at line 131)

  Add modes count alongside existing component counts.

  **Rationale:** Consistency between install prompt and info/list views.

## Verification Criteria

- `forge plugin install /path/to/marketplace/author` correctly resolves `marketplace.json` → installs only the `./plugin` subdirectory
- Trust prompt shows correct component counts: skills (7), commands (0), agents (0), hooks (present), MCP servers (1) for the claude-mem example
- Runtime `scan_root` of `~/.claude/plugins/` discovers plugins inside `marketplaces/<author>/` via `marketplace.json` indirection
- Hooks using `${CLAUDE_PLUGIN_ROOT}` in their commands execute correctly with the variable resolved to the plugin's directory path
- MCP servers with `${CLAUDE_PLUGIN_ROOT}` in their command field start correctly
- Existing Forge-native plugins and Claude Code flat-layout plugins continue to work without regression
- All existing plugin tests pass, plus new tests for marketplace layout and env var aliases
- `cargo check` and `cargo insta test --accept` pass

## Potential Risks and Mitigations

1. **Ambiguous manifests — repo-root vs nested plugin both have `.claude-plugin/plugin.json`**
   Mitigation: When marketplace.json is detected, the repo-root manifest is used only for metadata display; the nested plugin's manifest becomes the source of truth for component resolution. `scan_root` should only emit one `LoadedPlugin` per marketplace entry, not one for the repo root AND one for the nested plugin.

2. **Multiple plugins per marketplace — `marketplace.json` may list more than one plugin**
   Mitigation: Iterate all entries in `plugins[]`, resolving each `source` independently. Each becomes a separate `LoadedPlugin`. The install flow can prompt the user to select which plugin to install if there are multiple.

3. **Broken `source` paths — `marketplace.json` may point to non-existent directories**
   Mitigation: Validate that `<root>/<source>` exists and contains a manifest; surface a clear error if not.

4. **Performance — extra filesystem probes for marketplace.json on every scan**
   Mitigation: The extra `exists()` call per child directory is negligible compared to existing manifest probing (already 3 candidates per dir). Marketplace.json is only probed when no manifest is found directly.

5. **Env var pollution — adding `CLAUDE_*` aliases unconditionally**
   Mitigation: Phase 4 conditionally injects `CLAUDE_*` aliases only for `PluginSource::ClaudeCode` plugins. Phase 3 (`CLAUDE_PLUGIN_ROOT`) is added unconditionally as it's the most critical variable and the cost is negligible.

## Alternative Approaches

1. **Symlink-based resolution**: Instead of parsing `marketplace.json`, detect symlinks at `~/.claude/plugins/` that point into deeper directories. Rejected because Claude Code uses actual directory nesting, not symlinks.

2. **Recursive scan to arbitrary depth**: Make `scan_root` recurse until it finds manifests at any depth. Rejected because it's expensive and fragile — could accidentally discover manifests in `node_modules/` or test fixtures.

3. **Require users to point at the nested plugin directory**: Tell users to run `forge plugin install /path/to/author/plugin/` instead of the repo root. Rejected because it creates a poor UX and diverges from how Claude Code installs marketplace plugins.

4. **Transform Claude Code hooks into Forge format at install time**: Rewrite `${CLAUDE_PLUGIN_ROOT}` → `${FORGE_PLUGIN_ROOT}` in hooks.json during install. Rejected because it's fragile, breaks updates, and the original hooks.json should remain unmodified for Claude Code compatibility.
