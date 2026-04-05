# Helper function to select a provider from the list
# Port of _forge_select_provider from zsh.
# Usage: _forge_select_provider [filter_status] [current_provider] [filter_type] [query]
# Returns: selected provider line (via stdout)

function _forge_select_provider
    set -l filter_status ""
    set -l current_provider ""
    set -l filter_type ""
    set -l query ""

    if test (count $argv) -ge 1
        set filter_status $argv[1]
    end
    if test (count $argv) -ge 2
        set current_provider $argv[2]
    end
    if test (count $argv) -ge 3
        set filter_type $argv[3]
    end
    if test (count $argv) -ge 4
        set query $argv[4]
    end

    # Build the command with type filter if specified
    set -l cmd $_FORGE_BIN list provider --porcelain
    if test -n "$filter_type"
        set cmd $cmd --type=$filter_type
    end

    set -l output ($cmd 2>/dev/null | string collect)

    if test -z "$output"
        _forge_log error "No providers available"
        return 1
    end

    # Filter by status if specified (e.g., "[yes]" for confirmed providers)
    if test -n "$filter_status"
        # Preserve the header line and filter the rest
        set -l header (echo "$output" | head -n 1 | string collect)
        set -l filtered (echo "$output" | tail -n +2 | string match -r -- "$filter_status" | string collect)
        if test -z "$filtered"
            _forge_log error "No $filter_status providers found"
            return 1
        end
        set output (printf "%s\n%s" "$header" "$filtered" | string collect)
    end

    # Get current provider if not provided
    if test -z "$current_provider"
        set current_provider ($_FORGE_BIN config get provider --porcelain 2>/dev/null | string collect)
    end

    set -l fzf_args \
        --delimiter="$_FORGE_DELIMITER" \
        --prompt="Provider > " \
        --with-nth="1,3.."

    # Add query parameter if provided
    if test -n "$query"
        set fzf_args $fzf_args --query="$query"
    end

    # Position cursor on current provider if available
    if test -n "$current_provider"
        # For providers, compare against the first field (display name)
        set -l index (_forge_find_index "$output" "$current_provider" 1)
        set fzf_args $fzf_args --bind="start:pos($index)"
    end

    set -l selected (echo "$output" | _forge_fzf --header-lines=1 $fzf_args)

    if test -n "$selected"
        echo "$selected"
        return 0
    end

    return 1
end
