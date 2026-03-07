
# Added by forge zsh setup
if [ -t 0 ] && [ -x "{{zsh}}" ]; then
  export SHELL="{{zsh}}"
  exec "{{zsh}}"
fi
