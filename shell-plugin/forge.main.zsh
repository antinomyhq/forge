#!/usr/bin/env zsh

# Main loader for forge plugin modules
# This file sources all modular components in the correct order

# Get the directory where this script is located
local FORGE_PLUGIN_DIR="${0:A:h}"

# Source syntax highlighting configuration
source "${FORGE_PLUGIN_DIR}/forge.highlight.zsh"

# Source logging utilities
source "${FORGE_PLUGIN_DIR}/forge.log.zsh"

# Source prompt configuration
source "${FORGE_PLUGIN_DIR}/forge.prompt.zsh"
