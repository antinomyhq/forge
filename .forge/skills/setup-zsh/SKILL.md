---
name: setup-zsh
description: Install and configure the Forge ZSH plugin with all dependencies (fzf, fd, bat, zsh-syntax-highlighting). Use when users request to set up, install, or configure the Forge ZSH shell plugin, shell integration, or terminal enhancements.
---

# Setup ZSH Plugin

Install and configure the Forge ZSH plugin with proper dependency checking and idempotent setup.

## Workflow

### 1. Run Comprehensive Verification

**Always start by running the verification script:**

```bash
bash .forge/skills/setup-zsh/scripts/verify.sh
```

This returns tab-separated output with all status information:
- `Status`: Overall status (Complete/Incomplete)
- `OS`: Operating system (macos/linux)
- `Package Manager`: Available package manager (brew/apt/pacman/unknown)
- `Framework`: ZSH framework (oh-my-zsh/prezto/standalone)
- `fzf`, `fd`, `bat`, `forge`: Dependency installation status
- `Syntax Highlighting Installed/Configured`: zsh-syntax-highlighting status
- `Forge Plugin Sourced`: Whether Forge plugin is in ~/.zshrc
- `FORGE_BIN Set`: Whether FORGE_BIN environment variable is configured

### 2. Check Setup Status

If `Status` contains "Complete":
- DO NOT install anything
- DO NOT modify any files
- Display success message and STOP

If `Status` contains "Incomplete":
- Proceed with installation of missing components only

### 3. Install Missing Dependencies

Based on verification output, install only what's missing using the detected package manager:

**fzf** (required):
- brew: `brew install fzf`
- apt: `sudo apt install -y fzf`
- pacman: `sudo pacman -S fzf`

**fd** (required):
- brew: `brew install fd`
- apt: `sudo apt install -y fd-find`
- pacman: `sudo pacman -S fd`

**bat** (optional):
- brew: `brew install bat`
- apt: `sudo apt install -y bat`
- pacman: `sudo pacman -S bat`

### 4. Install zsh-syntax-highlighting

Only if not configured:

**For oh-my-zsh:**
```bash
if [ ! -d "${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/zsh-syntax-highlighting" ]; then
  git clone https://github.com/zsh-users/zsh-syntax-highlighting.git \
    ${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/zsh-syntax-highlighting
fi
```

**For standalone:**
```bash
mkdir -p ~/.zsh
if [ ! -d ~/.zsh/zsh-syntax-highlighting ]; then
  git clone https://github.com/zsh-users/zsh-syntax-highlighting.git \
    ~/.zsh/zsh-syntax-highlighting
fi
```

**For prezto:**
- Built-in module, just enable in `~/.zpreztorc`

### 5. Configure ZSH

Only if changes are needed:

1. **Create backup:**
```bash
cp ~/.zshrc ~/.zshrc.backup.$(date +%Y%m%d_%H%M%S)
```

2. **Add configurations based on framework:**

**Oh-My-Zsh:**
- Add `zsh-syntax-highlighting` to plugins array if not present
- Add `source <(${FORGE_BIN:-forge} extension zsh)` after oh-my-zsh.sh if not present

**Standalone:**
- Add `source ~/.zsh/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh` if not present
- Add `source <(${FORGE_BIN:-forge} extension zsh)` if not present

**Prezto:**
- Enable syntax-highlighting module in `~/.zpreztorc`
- Add `source <(${FORGE_BIN:-forge} extension zsh)` at end of `~/.zshrc`

### 6. Verify and Report

After making changes:
1. Run verification script again to confirm success
2. Report what was installed/configured
3. Provide next steps for user

## Output Messages

**If already configured:**
```
✓ Forge ZSH Plugin is Already Configured!

All required components are installed and configured.
Your setup is complete!

To start using:
- Open a new terminal, or run: source ~/.zshrc
- Type ':' and press Tab to see available commands
- Try: :new, :info, or : <prompt>

```

**If changes were made:**
```
✓ Forge ZSH Plugin Setup Complete!

Changes Made:
[List what was installed/configured]

Configuration:
- Backup created: ~/.zshrc.backup.[timestamp]

Next Steps:
1. Reload your shell: source ~/.zshrc
2. Try the plugin: type ':' and press Tab
3. Quick commands: :new, :info, : <prompt>


Troubleshooting:
- If issues occur, restore backup: cp ~/.zshrc.backup.[timestamp] ~/.zshrc
```

## Key Principles

- **Idempotent**: Check before modifying - don't change what's already configured
- **Minimal**: Only install/configure what's actually missing
- **Safe**: Always backup before modifying configuration files
- **Clear**: Report exactly what was done vs what was already present
