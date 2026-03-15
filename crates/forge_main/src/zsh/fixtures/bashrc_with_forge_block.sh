# My bashrc
export PATH=$PATH:/usr/local/bin

# >>> forge initialize >>>
if [ -t 0 ] && [ -x "/usr/bin/zsh" ]; then
  export SHELL="/usr/bin/zsh"
  exec "/usr/bin/zsh"
fi
# <<< forge initialize <<<

# More config
alias ll='ls -la'
