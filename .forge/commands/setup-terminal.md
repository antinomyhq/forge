---
name: setup-terminal
description: Automates the ZSH integration setup from docs
---

Automate ZSH integration for Forge. Reference: https://forgecode.dev/docs/zsh-support/

**Rules:** 
   - Prefer package managers to install dependencies
   - Always backup `~/.zshrc` before making changes to enable restoration in case of failures

## Step 0: Verify Current Setup

Check if ZSH integration is already configured:

1. Run: `zsh -i -c 'forge_verify_dependencies'` and install only the dependencies that are not installed
2. If command succeeds, inform user that setup is already complete and proceed to Step 5
3. If command fails or function doesn't exist, proceed with setup steps below

## Step 1: Detect Package Manager

Detect available package managers on the system and select the appropriate one for installing dependencies.

## Step 2: Install Dependencies

Use the detected package manager to install dependencies if not already present:

1. Run `forge extension zsh` or `npx forgecode@latest extension zsh` to output the ZSH plugin
2. Parse the output to identify all required dependencies eg. bat, zsh-syntax-highlighting etc.
3. Install each missing dependency using the detected package manager with appropriate non-interactive flags.

## Step 3: Configure ZSH

1. Check if `forge` is installed: `command -v forge`
2. If not installed, install forge using: `npm install -g --yes forgecode@latest` or detected package manager
3. Backup: `cp ~/.zshrc ~/.zshrc.backup.$(date +%Y%m%d_%H%M%S)`
4. Remove existing `FORGE_BIN` exports and Forge lines from ~/.zshrc
5. Add to ~/.zshrc in the following order:
   - **First:** Source ALL installed ZSH plugins (no exceptions - every plugin must load before Forge)
   - **Last:** Add Forge configuration
     - `export FORGE_BIN=forge`
     - `source <($FORGE_BIN extension zsh)`

## Step 4: Verify

1. Once setup is complete and correct then you should be able to run: `zsh -i -c 'forge_verify_dependencies'` to verify all necessary dependencies are installed.
2. If verification fails, try to install the dependencies that are not installed else consult troubleshooting guide on https://forgecode.dev/docs/zsh-support

## Step 5: Next Steps

After successful setup, guide the user:

1. Reload the shell with `source ~/.zshrc` or `exec zsh`

2. **Start using the `:` prompt**:
   ```
   : hello, can you help me debug this shell script?
   ```

3. **Try agent-specific prompts** (`:agent_name query`):
   ```
   :sage what are the performance implications of this database query?
   ```

4. **Use fuzzy file finding**:
   ```
   : explain the config in @fileName<Tab>
   ```

5. **Learn more features**: Visit https://forgecode.dev/docs/zsh-support

**Report:** Provide details on installations performed, backup location, verification results, and any issues resolved.
