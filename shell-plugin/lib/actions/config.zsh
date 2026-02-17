#!/usr/bin/env zsh

# Configuration action handlers (agent, provider, model, tools, skill)

# Action handler: Select agent
function _forge_action_agent() {
    local input_text="$1"
    
    echo
    
    # If an agent ID is provided directly, use it
    if [[ -n "$input_text" ]]; then
        local agent_id="$input_text"
        
        # Validate that the agent exists (skip header line)
        local agent_exists=$($_FORGE_BIN list agents --porcelain 2>/dev/null | tail -n +2 | grep -q "^${agent_id}\b" && echo "true" || echo "false")
        if [[ "$agent_exists" == "false" ]]; then
            _forge_log error "Agent '\033[1m${agent_id}\033[0m' not found"
            return 0
        fi
        
        # Set the agent as active
        _FORGE_ACTIVE_AGENT="$agent_id"
        
        # Print log about agent switching
        _forge_log success "Switched to agent \033[1m${agent_id}\033[0m"
        
        return 0
    fi
    
    # Get agents list
    local agents_output
    agents_output=$($_FORGE_BIN list agents --porcelain 2>/dev/null)
    
    if [[ -n "$agents_output" ]]; then
        # Get current agent ID
        local current_agent="$_FORGE_ACTIVE_AGENT"
        
        local sorted_agents="$agents_output"
        
        # Create prompt with current agent - show agent ID, title, provider, model and reasoning
        local prompt_text="Agent ❯ "
        local fzf_args=(
            --prompt="$prompt_text"
            --delimiter="$_FORGE_DELIMITER"
            --with-nth="1,2,4,5,6"
        )

        # If there's a current agent, position cursor on it
        if [[ -n "$current_agent" ]]; then
            local index=$(_forge_find_index "$sorted_agents" "$current_agent")
            fzf_args+=(--bind="start:pos($index)")
        fi

        local selected_agent
        # Use fzf without preview for simple selection like provider/model
        selected_agent=$(echo "$sorted_agents" | _forge_fzf --header-lines=1 "${fzf_args[@]}")
        
        if [[ -n "$selected_agent" ]]; then
            # Extract the first field (agent ID)
            local agent_id=$(echo "$selected_agent" | awk '{print $1}')
            
            # Set the selected agent as active
            _FORGE_ACTIVE_AGENT="$agent_id"
            
            # Print log about agent switching
            _forge_log success "Switched to agent \033[1m${agent_id}\033[0m"
            
        fi
    else
        _forge_log error "No agents found"
    fi
}

# Action handler: Select provider
function _forge_action_provider() {
    local input_text="$1"
    echo
    local selected
    # Only show LLM providers (exclude context_engine and other non-LLM types)
    # Pass input_text as query parameter for fuzzy search
    selected=$(_forge_select_provider "" "" "llm" "$input_text")
    
    if [[ -n "$selected" ]]; then
        # Extract the second field (provider ID) from the selected line
        # Format: "DisplayName  provider_id  host  type  status"
        local provider_id=$(echo "$selected" | awk '{print $2}')
        # Always use config set - it will handle authentication if needed
        _forge_exec config set provider "$provider_id"
    fi
}

# Action handler: Select model
function _forge_action_model() {
    local input_text="$1"
    _forge_select_and_set_config "list models" "model" "Model" "$($_FORGE_BIN config get model --porcelain)" "2,3.." "$input_text"
}

# Action handler: Sync workspace for codebase search
function _forge_action_sync() {
    echo
    # Execute sync with stdin redirected to prevent hanging
    # Sync doesn't need interactive input, so close stdin immediately
    _forge_exec workspace sync </dev/null
}

# Action handler: Show sync status of workspace files
function _forge_action_sync_status() {
    local input_text="${1:-.}"
    echo
    _forge_exec workspace status "$input_text"
}

# Action handler: Show workspace info with sync details
function _forge_action_sync_info() {
    local input_text="${1:-.}"
    echo
    _forge_exec workspace info "$input_text"
}

# Helper function to select and set config values with fzf
function _forge_select_and_set_config() {
    local show_command="$1"
    local config_flag="$2"
    local prompt_text="$3"
    local default_value="$4"
    local with_nth="${5:-}"  # Optional column selection parameter
    local query="${6:-}"     # Optional query parameter for fuzzy search
    (
        echo
        local output
        # Handle multi-word commands properly
        if [[ "$show_command" == *" "* ]]; then
            # Split the command into words and execute with --porcelain
            local cmd_parts=(${=show_command})
            output=$($_FORGE_BIN "${cmd_parts[@]}" --porcelain 2>/dev/null)
        else
            output=$($_FORGE_BIN "$show_command" --porcelain 2>/dev/null)
        fi
        
        if [[ -n "$output" ]]; then
            local selected
            local fzf_args=(--delimiter="$_FORGE_DELIMITER" --prompt="$prompt_text ❯ ")

            if [[ -n "$with_nth" ]]; then
                fzf_args+=(--with-nth="$with_nth")
            fi

            # Add query parameter if provided
            if [[ -n "$query" ]]; then
                fzf_args+=(--query="$query")
            fi

            if [[ -n "$default_value" ]]; then
                # For models, compare against the first field (model_id)
                local index=$(_forge_find_index "$output" "$default_value" 1)
                
                fzf_args+=(--bind="start:pos($index)")
                
            fi
            selected=$(echo "$output" | _forge_fzf --header-lines=1 "${fzf_args[@]}")

            if [[ -n "$selected" ]]; then
                local name="${selected%% *}"
                _forge_exec config set "$config_flag" "$name"
            fi
        fi
    )
}

# Action handler: Show tools
function _forge_action_tools() {
    echo
    # Ensure FORGE_ACTIVE_AGENT always has a value, default to "forge"
    local agent_id="${_FORGE_ACTIVE_AGENT:-forge}"
    _forge_exec list tools "$agent_id"
}

# Action handler: Show skills
function _forge_action_skill() {
    echo
    _forge_exec list skill
}
