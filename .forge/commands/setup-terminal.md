---
name: setup-terminal
description: Automates the ZSH integration setup from docs
---

Your Goal is to automate ZSH integration for Forge. Reference: https://forgecode.dev/docs/zsh-support/

**Rules:** No bash scripts. Prefer package managers. Backup ~/.zshrc. Loading order: plugins → Forge → zsh-syntax-highlighting (LAST).

## Step 0: Verify Current Setup

Check if ZSH integration is already configured:

1. Run: `zsh -i -c 'forge_verify_dependencies'` and install only the dependencies that not installed.
2. If command succeeds, inform user setup is already complete and move to step 5.
3. If command fails or function doesn't exist, proceed with setup steps below.

## Step 1: Detect Package Manager

Detect the package managers installed on the system and use the appropriate one to install dependencies.

## Step 2: Install Dependencies

Use detected package manager to install dependency if it's not installed already.
- zsh-syntax-highlighting
- zsh-autosuggestions
- fd (note: use `fd-find` on Ubuntu/Debian)
- fzf

## Step 3: Configure ZSH

1. Check if `forge` is installed: `command -v forge`
2. If not installed, install forge using: `npm i -g forgecode@latest` or detected package manager
3. Backup: `cp ~/.zshrc ~/.zshrc.backup.$(date +%Y%m%d_%H%M%S)`
4. Remove existing `FORGE_BIN` exports and Forge lines from ~/.zshrc
5. Add to ~/.zshrc:
   - `export FORGE_BIN=forge`
   - `source <($FORGE_BIN extension zsh)`
6. Ensure zsh-syntax-highlighting loads LAST if installed.

## Step 4: Verify

1. Run: `zsh -i -c 'forge_verify_dependencies'`
2. If fails, check https://forgecode.dev/docs/zsh-support/#troubleshooting

## Step 5: Next Steps

After successful setup, guide the user:

0. Reload the shell with `source ~/.zshrc` or `exec zsh` 

1. **Start using the `:` prompt**:
   ```
   : hello, can you help me debug this shell script?
   ```

2. **Try agent-specific prompts**: :<agent_name> query
   ```
   :sage what are the performance implications of this database query?
   ```

3. **Use fuzzy file finding**:
   ```
   : explain the config in @fileName<Tab>
   ```

4. **Learn more features**: Visit https://forgecode.dev/docs/zsh-support

**Report:** installations, backup location, verification results, issues resolved.
