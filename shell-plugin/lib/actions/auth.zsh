#!/usr/bin/env zsh

# Authentication action handlers

# Shell-native provider authentication helper
# Discovers auth requirements, prompts via fzf/read, calls CLI with args
function _forge_provider_auth() {
    local provider_id="$1"
    
    # Get auth info from the CLI
    local auth_info
    auth_info=$(_forge_exec provider auth-info "$provider_id" 2>&1 </dev/null)
    
    if [[ $? -ne 0 ]]; then
        echo "Error: Failed to get auth info for provider '$provider_id'" >&2
        echo "$auth_info" >&2
        return 1
    fi
    
    # Parse the auth info output (key=value format)
    local auth_methods url_params configured
    while IFS='=' read -r key value; do
        case "$key" in
            auth_methods) auth_methods="$value" ;;
            url_params) url_params="$value" ;;
            configured) configured="$value" ;;
        esac
    done <<< "$auth_info"
    
    # Convert comma-separated strings to arrays
    local -a auth_methods_array url_params_array
    IFS=',' read -rA auth_methods_array <<< "$auth_methods"
    IFS=',' read -rA url_params_array <<< "$url_params"
    
    # Select auth method (if multiple available)
    local selected_auth_method
    if [[ ${#auth_methods_array[@]} -eq 1 ]]; then
        selected_auth_method="${auth_methods_array[1]}"
    elif [[ ${#auth_methods_array[@]} -gt 1 ]]; then
        echo "Select authentication method for $provider_id:" >/dev/tty
        selected_auth_method=$(printf '%s\n' "${auth_methods_array[@]}" | fzf --height=10 --prompt="Auth method: " </dev/tty >/dev/tty)
        if [[ -z "$selected_auth_method" ]]; then
            echo "Cancelled" >&2
            return 1
        fi
    else
        echo "Error: No authentication methods available for $provider_id" >&2
        return 1
    fi
    
    # Convert auth method to kebab-case for CLI (api_key -> api-key)
    local auth_method_cli="${selected_auth_method//_/-}"

    # Build CLI arguments array with kebab-case auth method
    local -a cli_args
    cli_args=("provider" "login" "$provider_id" "--auth-method" "$auth_method_cli" "--set-active")

    # Handle different auth methods
    case "$selected_auth_method" in
        api_key)
            # Prompt for API key — must use /dev/tty explicitly because this
            # runs inside a ZLE widget where ZLE owns stdin in raw mode.
            local api_key
            echo -n "Enter your $provider_id API key: " >/dev/tty
            read -rs api_key </dev/tty
            echo >/dev/tty  # newline after hidden input

            if [[ -z "$api_key" ]]; then
                echo "Error: API key cannot be empty" >&2
                return 1
            fi

            cli_args+=("--api-key" "$api_key")

            # Prompt for URL parameters if required
            for param in "${url_params_array[@]}"; do
                [[ -z "$param" ]] && continue
                local param_value
                echo -n "Enter $param: " >/dev/tty
                read -r param_value </dev/tty
                if [[ -z "$param_value" ]]; then
                    echo "Error: $param cannot be empty" >&2
                    return 1
                fi
                cli_args+=("--param" "${param}=${param_value}")
            done
            ;;

        google_adc)
            # Google ADC is fully automatic - just need URL params
            for param in "${url_params_array[@]}"; do
                [[ -z "$param" ]] && continue
                local param_value
                echo -n "Enter $param: " >/dev/tty
                read -r param_value </dev/tty
                if [[ -z "$param_value" ]]; then
                    echo "Error: $param cannot be empty" >&2
                    return 1
                fi
                cli_args+=("--param" "${param}=${param_value}")
            done
            ;;

        oauth_device|codex_device)
            # Device flow is fully automatic - no user input needed
            # The Rust CLI handles everything (opens browser, displays code, polls)
            ;;

        oauth_code)
            # OAuth code flow: Rust CLI opens browser and shows URL
            # We need to wait for user to paste the code
            # For now, let the Rust CLI handle it interactively
            # TODO: In the future, we could implement a two-phase flow:
            #   1. Call with --init-only to get the auth URL
            #   2. Prompt user for code in shell
            #   3. Call with --auth-code to complete
            ;;

        *)
            echo "Warning: Unknown auth method '$selected_auth_method', falling back to interactive mode" >&2
            ;;
    esac
    
    # Execute the login command with all arguments
    _forge_exec "${cli_args[@]}"
}

# Action handler: Login to provider
function _forge_action_login() {
    local input_text="$1"
    echo
    local selected
    # Pass input_text as query parameter for fuzzy search
    selected=$(_forge_select_provider "" "" "" "$input_text")
    if [[ -n "$selected" ]]; then
        # Extract the second field (provider ID)
        local provider=$(echo "$selected" | awk '{print $2}')
        # Use shell-native auth flow
        _forge_provider_auth "$provider"
    fi
}

# Action handler: Logout from provider
function _forge_action_logout() {
    local input_text="$1"
    echo
    local selected
    # Pass input_text as query parameter for fuzzy search
    selected=$(_forge_select_provider "\[yes\]" "" "" "$input_text")
    if [[ -n "$selected" ]]; then
        # Extract the second field (provider ID)
        local provider=$(echo "$selected" | awk '{print $2}')
        _forge_exec provider logout "$provider"
    fi
}
