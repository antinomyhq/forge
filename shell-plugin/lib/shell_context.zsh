#!/usr/bin/env zsh

# Shell context tracking for forge plugin
# Captures the last command and its exit code so that forge can understand
# what the user was doing before asking for help.
#
# Uses ZSH's preexec/precmd hook arrays so that existing user hooks are
# not overwritten.

# Variables to store shell context
typeset -g _FORGE_LAST_COMMAND=""
typeset -g _FORGE_LAST_EXIT_CODE=""

# preexec runs just before a command is executed.
# We capture the command string here.
function _forge_preexec() {
    # $1 is the command string as typed by the user
    _FORGE_LAST_COMMAND="$1"
}

# precmd runs just before the prompt is drawn (i.e., after the previous
# command finishes). We capture the exit code here.
function _forge_precmd() {
    # $? is the exit code of the command that just finished
    _FORGE_LAST_EXIT_CODE="$?"
}

# Register hooks using ZSH hook arrays (non-destructive)
autoload -Uz add-zsh-hook
add-zsh-hook preexec _forge_preexec
add-zsh-hook precmd _forge_precmd
