# Forge Fish Plugin

A Fish shell plugin that provides the same `:command` shortcuts, fzf-powered completions, and right-prompt integration as the built-in ZSH plugin — for users who run [Fish](https://fishshell.com/) as their daily driver.

## Features

- **`:command` dispatch** — 40+ commands routed through a single Enter key override
- **fzf integration** — Tab completion for `@file` paths and `:command` names with live preview
- **Right prompt** — shows active agent, model, conversation, and reasoning effort (plays nice with starship and other prompt themes)
- **Session overrides** — switch models, providers, and reasoning effort on the fly without touching config files
- **CLI completions** — full tab completion for the `forge` binary itself, including nested subcommands

## Requirements

- [Forge Code](https://github.com/antinomyhq/forgecode) installed and on your `$PATH`
- [Fish](https://fishshell.com/) 3.4+
- [Fisher](https://github.com/jorgebucaran/fisher) (plugin manager)
- [fzf](https://github.com/junegunn/fzf)
- Optional: [fd](https://github.com/sharkdp/fd) for faster `@file` completion
- Optional: [bat](https://github.com/sharkdp/bat) for syntax-highlighted file previews

## Install

Via [Fisher](https://github.com/jorgebucaran/fisher):

```fish
fisher install FabioLissi/forge-fish
```

To update:

```fish
fisher update FabioLissi/forge-fish
```

To uninstall:

```fish
fisher remove FabioLissi/forge-fish
```

## Usage

### Quick reference

| Shortcut | What it does |
|---|---|
| `: <text>` | Send a prompt to Forge in the current conversation |
| `:new` / `:n` | Start a new conversation |
| `:suggest` / `:s` | AI generates a shell command from your description |
| `:commit` | AI writes a commit message and commits |
| `:commit-preview` | Preview the commit message without committing |
| `:model` / `:m` | Switch model for this session |
| `:agent` / `:a` | Switch between agents (forge, sage, muse, etc.) |
| `:conversation` / `:c` | Browse and switch conversations |
| `:edit` / `:ed` | Open your `$EDITOR` for a multi-line prompt |
| `:info` / `:i` | Show current session info |
| `:config` | Show configuration |
| `:copy` | Copy last response to clipboard |
| `:clone` | Clone a conversation |
| `:rename` / `:rn` | Rename the current conversation |
| `:compact` | Compact conversation context |
| `:retry` / `:r` | Retry the last command |
| `:dump` / `:d` | Export conversation as JSON or HTML |
| `:tools` / `:t` | List available tools |
| `:skill` | List available skills |
| `:sync` | Sync workspace for codebase search |
| `:doctor` | Run environment diagnostics |
| `:kb` | Show keyboard shortcuts reference |

### Tab completion

- Type `:` and press **Tab** to browse all commands with fzf
- Type `@` and press **Tab** to pick files from the current directory
- The `forge` CLI itself gets full tab completion (try `forge config set <Tab>`)

### Right prompt

The plugin appends Forge session info to your right prompt. If you use starship or another prompt tool, it wraps around it — your existing right prompt stays intact.

### Session overrides

Change settings for the current shell session without editing config files:

```fish
:model              # pick a model interactively
:reasoning-effort   # set reasoning effort (low/medium/high)
:config-reload      # reset all session overrides back to global config
```

## Plugin structure

This plugin follows the [Fisher](https://github.com/jorgebucaran/fisher) plugin layout:

```
fish/
├── completions/        # Tab completions for the forge CLI
│   └── forge.fish
├── conf.d/             # Auto-sourced on shell start
│   └── forge.fish      # Right prompt and session state init
└── functions/          # All :command handlers and helpers
    ├── forge_prompt.fish
    ├── _forge_*.fish   # Internal helper functions
    └── ...
```
