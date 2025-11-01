---
name: setup-terminal
description: Automates the ZSH integration setup from docs
---

Automate the 5-step ZSH integration process from https://forgecode.dev/docs/zsh-support/

**IMPORTANT**: 
  - Do NOT create bash scripts or intermediate files.
  - Check for package manager first before cloning repositories. Package manager installations are simpler and easier to maintain.

**Step 1: Install zsh-syntax-highlighting (Required)**
Priority order (check in this sequence):
1. **Package Manager** (PREFERRED on macOS/Linux):
   - macOS: `brew install zsh-syntax-highlighting`
   - Ubuntu/Debian: `apt install zsh-syntax-highlighting`
   - Arch: `pacman -S zsh-syntax-highlighting`
   - Fedora: `dnf install zsh-syntax-highlighting`
   
2. **Oh My Zsh Plugin** (only if package manager not available):
   ```bash
   git clone https://github.com/zsh-users/zsh-syntax-highlighting.git ${ZSH_CUSTOM:-~/.oh-my-zsh/custom}/plugins/zsh-syntax-highlighting
   ```
   Then add to plugins in ~/.zshrc: `plugins=(... zsh-syntax-highlighting)` (must be last plugin)

3. **Manual Installation** (last resort):
   ```bash
   git clone https://github.com/zsh-users/zsh-syntax-highlighting.git
   echo "source ${(q-)PWD}/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh" >> ${ZDOTDIR:-$HOME}/.zshrc
   ```

**Note**: When using package managers (brew/apt/etc), the sourcing is handled automatically. With Oh My Zsh plugins or manual installation, you must ensure it loads LAST in ~/.zshrc.

**Step 2: Install fd and fzf (Optional but Recommended)**
- Package manager: `brew install fd fzf` or `apt install fd-find fzf` or `pacman -S fd fzf`
- Note: Ubuntu/Debian uses `fd-find` package name
- Skip if installation fails - user can add later

**Step 3: Configure ZSH Integration**
Detect Forge installation type: Use `which forge` or `command -v forge` to check if installed globally, otherwise use `npx forgecode@latest`

Backup ~/.zshrc first, then add to it:
```bash
# If Forge is global
export FORGE_BIN=forge
source <($FORGE_BIN extension zsh)

# OR if using npx
export FORGE_BIN="npx forgecode@latest"
source <($FORGE_BIN extension zsh)
```
Remove any existing FORGE_BIN exports or Forge integration lines to avoid duplicates.

**Critical Loading Order:**
1. All other plugins
2. Forge integration (above lines)
3. zsh-syntax-highlighting source (MUST BE LAST)

**Step 4: Verify Installation**

**IMPORTANT**:
  - Before we run the verification steps, we've to reload shell with following commands.
    commands: `source ~/.zshrc` or `exec zsh`

Run verification tests:
1. **ZLE Widgets Registration**: Check if forge widgets are registered
   ```bash
   zsh -i -c 'zle -l | grep -E "forge-(accept-line|completion)"'
   ```
   Should return both `forge-accept-line` and `forge-completion`

2. **Syntax highlighting**: Check if `$ZSH_HIGHLIGHT_VERSION` is set (indicates zsh-syntax-highlighting loaded)

3. **fd/fzf integration** (if installed in Step 2):
   - Check `which fd` or `which fdfind` returns a path
   - Check `which fzf` returns a path
   - If both exist, fuzzy finding features will work

Report any test failures for troubleshooting.

**Report**: Show what was installed, backup location (if created).

**Auto-heal**: If any step fails, consult the Troubleshooting section at https://forgecode.dev/docs/zsh-support/, diagnose the specific issue, and apply the documented fixes.
