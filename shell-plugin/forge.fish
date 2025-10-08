#!/usr/bin/env fish

# Documentation in [README.md](./README.md)

# Configuration: Change these variables to customize the forge command and special characters
set -g _FORGE_BIN (type -q $FORGE_BIN; and echo $FORGE_BIN; or echo "forge")
set -g _FORGE_CONVERSATION_PATTERN ":"

# Detect fd command - Ubuntu/Debian use 'fdfind', others use 'fd'
set -g _FORGE_FD_CMD (command -v fdfind 2>/dev/null; or command -v fd 2>/dev/null; or echo '')

# Detect fzf availability
set -g _FORGE_HAS_FZF (type -q fzf; and echo 1; or echo 0)

# Cache the commands list once at plugin load time
set -g _FORGE_COMMANDS (eval $_FORGE_BIN show-commands 2>/dev/null)

# Store conversation ID and active agent
set -gx FORGE_CONVERSATION_ID ""
set -gx FORGE_ACTIVE_AGENT "forge"

# Helper function to execute forge commands consistently
function _forge_exec
    eval $_FORGE_BIN (string escape -- $argv)
end

# Helper function to print operating agent messages with consistent formatting
function _forge_print_agent_message
    set -l agent_name $argv[1]
    if test -z "$agent_name"
        set agent_name $FORGE_ACTIVE_AGENT
    end
    set -l timestamp (date '+%H:%M:%S')
    set -l upper_agent (string upper $agent_name)
    echo -e "\033[33m⏺\033[0m \033[90m[$timestamp] \033[1;37m$upper_agent\033[0m \033[90mis now the active agent\033[0m"
end

# Helper function to select and set config values with fzf
function _forge_select_and_set_config
    set -l show_command $argv[1]
    set -l config_flag $argv[2]
    set -l prompt_text $argv[3]
    
    echo
    set -l output (eval $_FORGE_BIN $show_command 2>/dev/null)
    
    if test -n "$output"
        set -l selected
        
        if test $_FORGE_HAS_FZF -eq 1
            # Use fzf for interactive selection
            set selected (echo "$output" | fzf --cycle --select-1 --height 40% --reverse --prompt="$prompt_text ❯ ")
        else
            # Fallback: number the options and use Fish's read
            echo "$output" | awk '{print NR ". " $0}'
            echo ""
            read -l -P "$prompt_text (enter number): " choice
            
            if test -n "$choice"; and string match -qr '^\d+$' -- $choice
                set selected (echo "$output" | sed -n "$choice p")
            end
        end
        
        if test -n "$selected"
            set -l name (echo "$selected" | awk '{print $1}')
            _forge_exec config set --$config_flag $name
        end
    end
end

# Helper function to handle session commands that require an active conversation
function _forge_handle_session_command
    set -l subcommand $argv[1]
    set -l extra_args $argv[2..]
    
    echo
    
    # Check if FORGE_CONVERSATION_ID is set
    if test -z "$FORGE_CONVERSATION_ID"
        echo -e "\033[31m✗\033[0m No active conversation. Start a conversation first or use :list to see existing ones"
        commandline -f repaint
        return 0
    end
    
    # Execute the session command with conversation ID and any extra arguments
    _forge_exec session --id $FORGE_CONVERSATION_ID $subcommand $extra_args
    
    commandline -f repaint
    return 0
end

# Action handler: Start a new conversation
function _forge_action_new
    echo
    _forge_exec show-banner
    _forge_print_agent_message "FORGE"
    set -g FORGE_CONVERSATION_ID ""
    set -g FORGE_ACTIVE_AGENT "forge"
    commandline ""
    commandline -f repaint
end

# Action handler: Show info
function _forge_action_info
    echo
    _forge_exec info
    commandline ""
    commandline -f repaint
end

# Action handler: Dump conversation
function _forge_action_dump
    set -l input_text $argv[1]
    if test "$input_text" = "html"
        _forge_handle_session_command dump html
    else
        _forge_handle_session_command dump
    end
    commandline ""
end

# Action handler: Compact conversation
function _forge_action_compact
    _forge_handle_session_command compact
    commandline ""
end

# Action handler: Retry last message
function _forge_action_retry
    _forge_handle_session_command retry
    commandline ""
end

# Action handler: List/switch conversations
function _forge_action_conversation
    echo
    
    # Get conversations list
    set -l conversations_output (eval $_FORGE_BIN session --list 2>/dev/null)
    
    if test -n "$conversations_output"
        # Get current conversation ID if set
        set -l current_id $FORGE_CONVERSATION_ID
        
        # Create prompt with current conversation
        set -l prompt_text "Conversation ❯ "
        if test -n "$current_id"
            set prompt_text "Conversation [Current: $current_id] ❯ "
        end
        
        set -l selected_conversation
        
        if test $_FORGE_HAS_FZF -eq 1
            # Use fzf for interactive selection
            set selected_conversation (echo "$conversations_output" | fzf --cycle --select-1 --height 40% --reverse --prompt="$prompt_text")
        else
            # Fallback: number the options and use Fish's read
            echo "$conversations_output" | awk '{print NR ". " $0}'
            echo ""
            read -l -P "$prompt_text (enter number): " choice
            
            if test -n "$choice"; and string match -qr '^\d+$' -- $choice
                set selected_conversation (echo "$conversations_output" | sed -n "$choice p")
            end
        end
        
        if test -n "$selected_conversation"
            # Strip ANSI codes first, then extract the last field (UUID)
            set -l conversation_id (echo "$selected_conversation" | sed 's/\x1b\[[0-9;]*m//g' | sed 's/\x1b\[K//g' | awk '{print $NF}' | tr -d '\n')
            
            # Set the selected conversation as active
            set -g FORGE_CONVERSATION_ID $conversation_id
            
            set -l timestamp (date '+%H:%M:%S')
            echo -e "\033[36m⏺\033[0m \033[90m[$timestamp] Switched to conversation \033[1m$conversation_id\033[0m"
        end
    else
        echo -e "\033[31m✗\033[0m No conversations found"
    end
    
    commandline ""
    commandline -f repaint
end

# Action handler: Select provider
function _forge_action_provider
    _forge_select_and_set_config show-providers provider "Provider"
    commandline ""
    commandline -f repaint
end

# Action handler: Select model
function _forge_action_model
    _forge_select_and_set_config show-models model "Model"
    commandline ""
    commandline -f repaint
end

# Action handler: Show tools
function _forge_action_tools
    echo
    _forge_exec show-tools $FORGE_ACTIVE_AGENT
    commandline ""
    commandline -f repaint
end

# Action handler: Set active agent or execute command
function _forge_action_default
    set -l user_action $argv[1]
    set -l input_text $argv[2]
    
    # Validate that the command exists in show-commands (if user_action is provided)
    if test -n "$user_action"
        if test -n "$_FORGE_COMMANDS"
            # Check if the user_action is in the list of valid commands
            if not echo "$_FORGE_COMMANDS" | grep -q "^$user_action\b"
                echo
                set -l timestamp (date '+%H:%M:%S')
                echo -e "\033[31m⏺\033[0m \033[90m[$timestamp]\033[0m \033[1;31mERROR:\033[0m Command '\033[1m$user_action\033[0m' not found"
                commandline ""
                commandline -f repaint
                return 0
            end
        end
    end
    
    # If input_text is empty, just set the active agent
    if test -z "$input_text"
        echo
        if test -n "$user_action"
            set -g FORGE_ACTIVE_AGENT $user_action
        end
        _forge_print_agent_message
        commandline ""
        commandline -f repaint
        return 0
    end
    
    # Generate conversation ID if needed
    if test -z "$FORGE_CONVERSATION_ID"
        set -g FORGE_CONVERSATION_ID (eval $_FORGE_BIN --generate-conversation-id)
    end
    
    # Set the active agent for this execution
    if test -n "$user_action"
        set -g FORGE_ACTIVE_AGENT $user_action
    end
    
    echo
    
    # Execute the forge command directly with proper escaping
    _forge_exec -p $input_text
    
    # Reset the command line
    commandline ""
    commandline -f repaint
end

# Custom completion function for @ and :command
function _forge_completion
    set -l cmd (commandline -cp)
    set -l current_token (commandline -ct)
    
    # Handle @ completion for file tagging
    if string match -qr '^@' -- $current_token
        set -l filter_text (string sub -s 2 -- $current_token)
        set -l selected
        
        # Check if we have both fd and fzf
        if test -z "$_FORGE_FD_CMD"
            echo ""
            echo -e "\033[33m⚠\033[0m fd/fdfind not found. Install it for file tagging support:"
            echo "  macOS: brew install fd"
            echo "  Ubuntu/Debian: sudo apt install fd-find"
            commandline -f repaint
            return 0
        end
        
        if test $_FORGE_HAS_FZF -eq 1
            # Use fzf for interactive selection
            if test -n "$filter_text"
                set selected (eval $_FORGE_FD_CMD --type f --hidden --exclude .git | fzf --cycle --select-1 --height 40% --reverse --query "$filter_text")
            else
                set selected (eval $_FORGE_FD_CMD --type f --hidden --exclude .git | fzf --cycle --select-1 --height 40% --reverse)
            end
        else
            # Fallback: use find and simple list
            echo ""
            echo "Available files (type path or Ctrl+C to cancel):"
            if test -n "$filter_text"
                eval $_FORGE_FD_CMD --type f --hidden --exclude .git | grep -i "$filter_text" | head -20
            else
                eval $_FORGE_FD_CMD --type f --hidden --exclude .git | head -20
            end
            echo ""
            read -l -P "Enter file path: " selected
        end
        
        if test -n "$selected"
            set -l tagged_file "@[$selected]"
            # Replace the current token with the tagged file
            set -l prefix (commandline -cp | string replace -r '@[^]]*$' '')
            commandline -r "$prefix$tagged_file"
            commandline -f repaint
        end
        return 0
    end
    
    # Handle :command completion
    if string match -qr '^:[a-zA-Z]*$' -- $cmd
        # Extract the text after the colon for filtering
        set -l filter_text (string sub -s 2 -- $cmd)
        
        # Use the cached commands list
        if test -n "$_FORGE_COMMANDS"
            set -l selected
            
            if test $_FORGE_HAS_FZF -eq 1
                # Use fzf for interactive selection with prefilled filter
                if test -n "$filter_text"
                    set selected (echo "$_FORGE_COMMANDS" | fzf --cycle --select-1 --height 40% --reverse --nth=1 --query "$filter_text" --prompt="Command ❯ ")
                else
                    set selected (echo "$_FORGE_COMMANDS" | fzf --cycle --select-1 --height 40% --reverse --nth=1 --prompt="Command ❯ ")
                end
            else
                # Fallback: number the options and use Fish's read
                echo ""
                if test -n "$filter_text"
                    echo "$_FORGE_COMMANDS" | grep -i "$filter_text" | awk '{print NR ". " $0}'
                else
                    echo "$_FORGE_COMMANDS" | awk '{print NR ". " $0}'
                end
                echo ""
                read -l -P "Select command (enter number): " choice
                
                if test -n "$choice"; and string match -qr '^\d+$' -- $choice
                    if test -n "$filter_text"
                        set selected (echo "$_FORGE_COMMANDS" | grep -i "$filter_text" | sed -n "$choice p")
                    else
                        set selected (echo "$_FORGE_COMMANDS" | sed -n "$choice p")
                    end
                end
            end
            
            if test -n "$selected"
                # Extract just the command name (first word before any description)
                set -l command_name (echo "$selected" | awk '{print $1}')
                # Replace the current buffer with the selected command
                commandline -r ":$command_name "
                commandline -f repaint
            end
        end
        return 0
    end
    
    # Fall back to default completion
    commandline -f complete
end

# Main function to handle :command execution
function _forge_accept_line
    set -l buffer (commandline)
    
    # Parse the buffer
    set -l user_action ""
    set -l input_text ""
    
    # Check if the line starts with any of the supported patterns
    if string match -qr '^:([a-zA-Z][a-zA-Z0-9_-]*)( (.*))?$' -- $buffer
        # Action with or without parameters: :foo or :foo bar baz
        set -l matches (string match -r '^:([a-zA-Z][a-zA-Z0-9_-]*)( (.*))?$' -- $buffer)
        set user_action $matches[2]
        if test (count $matches) -ge 4
            set input_text $matches[4]
        else
            set input_text ""
        end
    else if string match -qr '^: (.*)$' -- $buffer
        # Default action with parameters: : something
        set -l matches (string match -r '^: (.*)$' -- $buffer)
        set user_action ""
        set input_text $matches[2]
    else
        # For non-:commands, use normal behavior
        commandline -f execute
        return
    end
    
    # Handle aliases - convert to their actual agent names
    switch $user_action
        case ask
            set user_action sage
        case plan
            set user_action muse
    end
    
    # Dispatch to appropriate action handler using pattern matching
    switch $user_action
        case new n
            _forge_action_new
        case info i
            _forge_action_info
        case dump
            _forge_action_dump $input_text
        case compact
            _forge_action_compact
        case retry
            _forge_action_retry
        case conversation
            _forge_action_conversation
        case provider
            _forge_action_provider
        case model
            _forge_action_model
        case tools
            _forge_action_tools
        case '*'
            _forge_action_default $user_action $input_text
    end
end

# Bind Enter to our custom accept-line that transforms :commands
bind \r _forge_accept_line
bind \n _forge_accept_line

# Bind Tab to our custom completion
bind \t _forge_completion

# Fish syntax highlighting for tagged files and conversation patterns
# Note: Fish doesn't have the same pattern-based highlighting as zsh-syntax-highlighting
# You may need to implement custom highlighting using fish_color_* variables if desired
