# My bashrc
export PATH=$PATH:/usr/local/bin

# Added by zsh installer
if [ -t 0 ] && [ -x "/usr/bin/zsh" ]; then
  export SHELL="/usr/bin/zsh"

# Added by forge zsh setup
if [ -t 0 ] && [ -x "/usr/bin/zsh" ]; then
  export SHELL="/usr/bin/zsh"
  exec "/usr/bin/zsh"
