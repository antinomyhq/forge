
# --- zsh installer fpath (added by forge zsh setup) ---
_zsh_fn_base="/usr/share/zsh/functions"
if [ -d "$_zsh_fn_base" ]; then
  fpath=("$_zsh_fn_base" $fpath)
  for _zsh_fn_sub in "$_zsh_fn_base"/*/; do
    [ -d "$_zsh_fn_sub" ] && fpath=("${_zsh_fn_sub%/}" $fpath)
  done
fi
unset _zsh_fn_base _zsh_fn_sub
# --- end zsh installer fpath ---
