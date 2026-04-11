# Plugin System Finalization

## Objective

Complete the remaining plugin system integration gaps:
1. Wire 10 `#[allow(dead_code)] // TODO` methods into the hot-reload and session cleanup paths
2. Add MCP and hook config invalidation to `reload_plugin_components`
3. Add `:plugin` command support to the ZSH shell-plugin so it works from the terminal

## Context

The plugin system is architecturally complete with 24/27 lifecycle events wired, full discovery, CLI commands, hook dispatch, and test coverage (unit, integration, performance). What remains is "last-mile" wiring: connecting already-written-and-tested methods into their call sites, and exposing the `:plugin` command through the shell plugin.

---

## Implementation Plan

### Group A: Hot-Reload Completeness in `reload_plugin_components`

**Rationale**: The blanket `PluginComponentsReloader` impl at `crates/forge_app/src/services.rs:1260-1275` currently invalidates 4 caches (plugin loader, skill fetch, agent registry, command loader) but omits 2 critical ones: MCP servers contributed by plugins, and the hook config merged from plugin `hooks.json` files. When a user runs `:plugin enable/disable/reload`, plugin-contributed MCP servers and hooks remain stale until restart.

- [ ] **A1. Add hook config loader invalidation to `reload_plugin_components`**
  - In `crates/forge_app/src/services.rs:1260-1275`, add step 5: `self.hook_config_loader().invalidate().await?;`
  - This calls the existing `HookConfigLoaderService::invalidate()` method defined at `crates/forge_app/src/hook_runtime.rs:106`
  - After this, the next hook dispatch will re-merge user/project/plugin hooks from disk
  - Place after step 4 (command reload) since hook config depends on fresh plugin discovery results

- [ ] **A2. Add MCP service reload to `reload_plugin_components`**
  - In `crates/forge_app/src/services.rs:1260-1275`, add step 6: `self.mcp_service().reload_mcp().await?;`
  - This calls the existing `McpService::reload_mcp()` method; the `refresh_cache()` impl at `crates/forge_services/src/mcp/service.rs:198-205` clears the infra cache, config hash, tool map, and failed servers
  - Placing this last avoids interactive OAuth prompts during reload (MCP connections are lazy)

- [ ] **A3. Remove redundant `reload_mcp` call from `on_plugin_toggle`**
  - At `crates/forge_main/src/ui.rs:4655`, `on_plugin_toggle` already calls `self.api.reload_plugins()` which will now (after A2) include MCP reload
  - Verify there is no separate `reload_mcp` call in `on_plugin_toggle` that would double-fire; currently there is none in toggle but there is a standalone one at `ui.rs:1181` in a different path (MCP login) — leave that one alone

### Group B: Skill Listing Delta Cache Reset on Hot-Reload

**Rationale**: `SkillListingHandler` maintains a per-conversation delta cache that tracks which skills have already been announced to the LLM. When plugins change (new skills appear or old ones disappear), the delta cache must be reset so every active conversation re-announces the full catalog. The methods `reset_all()` and `reset_sent_skills()` are written and tested but marked `dead_code`.

- [ ] **B1. Expose `SkillListingHandler` reset through a new `PluginHookHandler` method**
  - The challenge: `SkillListingHandler` is owned by the orchestrator (not by `Services`), as noted in the trait doc at `crates/forge_app/src/services.rs:691-695`
  - The cleanest path: add a method `reset_skill_listing_caches()` on `PluginHookHandler` (or on the orchestrator's hook chain) that calls `skill_listing_handler.reset_all().await`
  - Alternative: add a `PluginReloadObserver` trait that the orchestrator implements, invoked by the API layer after `reload_plugin_components()`
  - Decision: use the simpler approach — `PluginHookHandler` already has access to `services: Arc<S>`, and since `SkillListingHandler` is not accessible from there, the UI layer (`on_plugin_toggle`, `on_plugin_reload`) should directly call `reset_all()` on whatever handle it has to the skill listing handler
  - This requires the UI to hold a reference to (or be able to reach) the `SkillListingHandler`

- [ ] **B2. Wire `reset_all()` call into `on_plugin_reload` at `crates/forge_main/src/ui.rs:4730-4738`**
  - After `self.api.reload_plugins().await?`, call the skill listing handler's `reset_all()`
  - This ensures every active conversation re-announces the full skill catalog on its next turn

- [ ] **B3. Wire `reset_all()` call into `on_plugin_toggle` at `crates/forge_main/src/ui.rs:4638-4660`**
  - Same pattern as B2 — after `reload_plugins`, reset the delta cache

- [ ] **B4. Remove `#[allow(dead_code)]` from wired methods**
  - `crates/forge_app/src/hooks/skill_listing.rs:213` — `DeltaCache::forget()`
  - `crates/forge_app/src/hooks/skill_listing.rs:227` — `DeltaCache::forget_all()`
  - `crates/forge_app/src/hooks/skill_listing.rs:314` — `reset_sent_skills()`
  - `crates/forge_app/src/hooks/skill_listing.rs:326` — `reset_all()`

### Group C: PluginHookHandler Hot-Reload Accessors

**Rationale**: Three methods on `PluginHookHandler` are `dead_code` — they're builder/accessor methods intended for hot-reload (plugin enable/disable) and session lifecycle wiring.

- [ ] **C1. Wire `with_session_hooks()` into the session creation path**
  - `crates/forge_app/src/hooks/plugin.rs:107` — `with_session_hooks(services, session_hooks)`
  - This constructor is meant for creating a `PluginHookHandler` that shares a `SessionHookStore` with the orchestrator
  - Evaluate whether the current `PluginHookHandler::new()` / `with_env_cache()` constructors used in production already cover this case; if so, determine whether `with_session_hooks` is truly needed or can be removed
  - If the session hook store should be shared, the orchestrator creation path needs to use this constructor instead of `new()`

- [ ] **C2. Wire `session_env_cache()` accessor or confirm it's unused**
  - `crates/forge_app/src/hooks/plugin.rs:135` — returns `&SessionEnvCache`
  - If the shell service already receives the env cache via `with_env_cache()`, this accessor may be redundant
  - Decision: if `with_env_cache()` is the production constructor and the cache is passed at construction time, this accessor can be removed rather than wired

- [ ] **C3. Wire `session_hook_store()` accessor or confirm it's unused**
  - `crates/forge_app/src/hooks/plugin.rs:141` — returns `&SessionHookStore`
  - Same evaluation as C2: if no external caller needs runtime access to the store, remove rather than wire

### Group D: SessionHookStore Lifecycle Cleanup

**Rationale**: `SessionHookStore` has three `dead_code` methods: `add_hook()`, `clear_session()`, `has_hooks()`. The store is already integrated into dispatch (via `get_hooks()`), but session cleanup and dynamic registration are not wired.

- [ ] **D1. Wire `clear_session()` into `SessionEnd` handler**
  - At `crates/forge_app/src/hooks/plugin.rs:700-722`, the `SessionEnd` EventHandle impl fires session-end hooks but doesn't clean up session-scoped hooks
  - After dispatching `SessionEnd`, call `self.session_hooks.clear_session(&event.session_id).await` to prevent unbounded memory growth
  - This addresses the memory leak noted in the prior analysis

- [ ] **D2. Evaluate `add_hook()` — defer or remove**
  - `crates/forge_app/src/hooks/session_hooks.rs:53` — `add_hook()` enables runtime hook registration
  - Currently no production code dynamically registers hooks at runtime
  - Decision: keep the method and its `dead_code` annotation, documenting it as a future extension point for dynamic hook registration (e.g., from agent hooks or MCP tool outputs)
  - Alternative: if the codebase policy is to remove unused code, remove `add_hook()` and `has_hooks()` and re-add when needed

- [ ] **D3. Remove `#[allow(dead_code)]` from `clear_session` after D1**
  - `crates/forge_app/src/hooks/session_hooks.rs:98`

### Group E: ZSH Shell Plugin `:plugin` Command

**Rationale**: `:plugin list` in the ZSH shell mode fails because: (1) `plugin` is not in `built_in_commands.json`, (2) `dispatcher.zsh` has no `plugin` case, (3) there is no `_forge_action_plugin` handler. The TUI mode handles `/plugin` via `SlashCommand::Plugin` at `crates/forge_main/src/model.rs:526`, but the shell plugin uses a completely separate dispatch path.

- [ ] **E1. Add `plugin` entry to `built_in_commands.json`**
  - In `crates/forge_main/src/built_in_commands.json`, add:
    ```json
    {
      "command": "plugin",
      "description": "Manage plugins: list, enable, disable, info, reload, install"
    }
    ```
  - This makes `:plugin` discoverable via `forge list commands --porcelain` and tab-completion

- [ ] **E2. Add `plugin` case to `dispatcher.zsh`**
  - In `shell-plugin/lib/dispatcher.zsh:144-256`, add a case entry before the `*` wildcard:
    ```
    plugin|pl)
        _forge_action_plugin "$input_text"
    ;;
    ```
  - Alias `pl` follows the existing pattern of short aliases (`i`, `n`, `c`, `t`, etc.)

- [ ] **E3. Create `_forge_action_plugin` handler**
  - New file: `shell-plugin/lib/actions/plugin.zsh`
  - The handler should parse subcommands from `$input_text`: `list`, `enable <name>`, `disable <name>`, `info <name>`, `reload`, `install <path>`
  - For `list`, `info`, `reload`: delegate to `_forge_exec plugin <subcommand> [args]` (non-interactive)
  - For `enable`, `disable`: delegate to `_forge_exec plugin <subcommand> <name>` (non-interactive)
  - For `install`: delegate to `_forge_exec_interactive plugin install <path>` (interactive — trust prompt needs TTY)
  - Default (no subcommand): show `list`
  - Pattern reference: `_forge_action_skill` at `shell-plugin/lib/actions/config.zsh:504-507` for the simplest case; `_forge_action_conversation` at `shell-plugin/lib/actions/conversation.zsh:46` for subcommand parsing

- [ ] **E4. Source the new plugin action file**
  - Ensure `shell-plugin/lib/actions/plugin.zsh` is sourced by the plugin loader
  - Check `shell-plugin/forge.plugin.zsh` or equivalent loader file and add the source line following the pattern of existing action files

- [ ] **E5. Add the CLI `plugin` subcommand to the Rust binary**
  - Currently `/plugin` works in the TUI via `SlashCommand::Plugin`, but `forge plugin list` may not work as a CLI subcommand
  - Verify whether `forge plugin list` (non-TUI) is supported; if not, the shell plugin's `_forge_exec plugin list` calls will fail
  - If unsupported, the shell handler should use `_forge_exec_interactive` and route through the REPL's `/plugin` slash command, or add a proper CLI subcommand

---

## Verification Criteria

- After A1+A2: `:plugin enable/disable/reload` updates MCP server list and hook config in the same session without restart
- After B1-B4: enabling a plugin that provides new skills causes LLM to see the updated catalog on the next turn
- After D1: running multiple sessions doesn't leak `SessionHookStore` memory (entries cleaned on SessionEnd)
- After E1-E5: `:plugin list`, `:plugin enable <name>`, `:plugin disable <name>`, `:plugin info <name>`, `:plugin reload`, `:plugin install <path>` all work from ZSH shell mode
- All existing tests continue to pass (`cargo insta test --accept`)
- No remaining `#[allow(dead_code)] // TODO` annotations for methods that have been wired

## Potential Risks and Mitigations

1. **MCP reload in `reload_plugin_components` may trigger OAuth prompts**
   Mitigation: `McpService::refresh_cache()` at `crates/forge_services/src/mcp/service.rs:198-205` deliberately clears the cache without eagerly connecting — connections are lazy. Verify this contract holds.

2. **SkillListingHandler is owned by orchestrator, not by Services**
   Mitigation: The reset call must flow from the UI layer (which owns the orchestrator) rather than from `reload_plugin_components`. This is documented in the trait at `crates/forge_app/src/services.rs:691-695`. Group B accounts for this architectural constraint.

3. **`forge plugin list` may not exist as a CLI subcommand**
   Mitigation: E5 explicitly flags this. If it doesn't exist, the shell handler can either: (a) use `_forge_exec_interactive -p "/plugin list"` to route through the REPL, or (b) a proper CLI subcommand is added. Option (a) is simpler but less clean; option (b) is the correct long-term solution.

4. **Removing `dead_code` annotations may cause new compiler warnings**
   Mitigation: Only remove annotations for methods that are actually wired in the same PR. Methods kept as future extension points (e.g., `add_hook()`) retain their `#[allow(dead_code)]` with updated comments.

5. **Session hook cleanup race condition**
   Mitigation: `clear_session()` is called after the SessionEnd dispatch completes (not during), so all SessionEnd hooks have finished before the cleanup runs. The `RwLock` ensures thread safety.

## Alternative Approaches

1. **For Group B (SkillListingHandler reset)**: Instead of threading the reset through the UI layer, introduce a `PluginReloadNotifier` event bus that the orchestrator subscribes to. More decoupled but adds complexity for a single call site.

2. **For Group E (Shell plugin)**: Instead of creating a dedicated `_forge_action_plugin` handler, let `:plugin` fall through to `_forge_action_default` and add `plugin` to `built_in_commands.json` with type `BUILTIN`. This would require the Rust binary to support `forge plugin list` as a CLI subcommand (risk E5). Simpler shell code but requires more Rust changes.

3. **For Group D (SessionHookStore)**: Remove `add_hook()`, `has_hooks()`, and `clear_session()` entirely since they're unused in production. Simpler codebase but loses the tested infrastructure for future dynamic hook registration.
