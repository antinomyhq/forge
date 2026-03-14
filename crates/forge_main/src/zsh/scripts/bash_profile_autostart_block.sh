
# >>> forge initialize >>>
# Source ~/.bashrc for user customizations (aliases, functions, etc.)
if [ -f "$HOME/.bashrc" ]; then
  source "$HOME/.bashrc"
fi
# Auto-start zsh for interactive sessions
if [ -t 0 ] && [ -x "{{zsh}}" ]; then
  export SHELL="{{zsh}}"
  exec "{{zsh}}"
fi
# <<< forge initialize <<<
