#!/usr/bin/env zsh

# Shell action handler for :rename command
function _forge_action_rename() {
    local input_text="$1"
    
    echo
    
    # Simple UUID pattern matching (8-4-4-4-12 format)
    local uuid_pattern="[0-9a-f]\{8\}-[0-9a-f]\{4\}-[0-9a-f]\{4\}-[0-9a-f]\{12\}"
    
    # If an ID is provided directly, use it
    if [[ -n "$input_text" ]]; then
        # Check if input contains both ID and new title
        if [[ "$input_text" =~ $uuid_pattern ]]; then
            local conversation_id="$match"
            local new_title="${input_text#$conversation_id }"
            
            if [[ -z "$new_title" ]]; then
                echo "Error: No new title provided"
                return 1
            fi
            
            # Execute rename command
            forge conversation rename "$conversation_id" "$new_title"
            return $?
        else
            echo "Error: Invalid conversation ID format"
            return 1
        fi
    fi
    
    # If no ID provided, show usage
    echo "Usage: :rename <conversation-id> <new-title>"
    echo "Example: :rename 123e4567-e89b-12d3-a456-426614174000 My New Title"
    return 1
}