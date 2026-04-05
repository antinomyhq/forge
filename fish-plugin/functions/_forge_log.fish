# Print messages with consistent formatting based on log level
# Color scheme matches crates/forge_main/src/title_display.rs
# Usage: _forge_log <level> <message>
# Levels: error, info, success, warning, debug

function _forge_log
    set -l level $argv[1]
    set -l message $argv[2]
    set -l timestamp (set_color 888888)"["(date '+%H:%M:%S')"]"(set_color normal)

    switch $level
        case error
            # Category::Error - Red circle
            printf '%s%s%s %s %s%s%s\n' \
                (set_color red) '⏺' (set_color normal) \
                "$timestamp" \
                (set_color red) "$message" (set_color normal)
        case info
            # Category::Info - White circle
            printf '%s%s%s %s %s%s%s\n' \
                (set_color white) '⏺' (set_color normal) \
                "$timestamp" \
                (set_color white) "$message" (set_color normal)
        case success
            # Category::Action/Completion - Yellow circle
            printf '%s%s%s %s %s%s%s\n' \
                (set_color yellow) '⏺' (set_color normal) \
                "$timestamp" \
                (set_color white) "$message" (set_color normal)
        case warning
            # Category::Warning - Bright yellow warning sign
            printf '%s%s%s %s %s%s%s\n' \
                (set_color bryellow) '⚠️' (set_color normal) \
                "$timestamp" \
                (set_color bryellow) "$message" (set_color normal)
        case debug
            # Category::Debug - Cyan circle with dimmed text
            printf '%s%s%s %s %s%s%s\n' \
                (set_color cyan) '⏺' (set_color normal) \
                "$timestamp" \
                (set_color 888888) "$message" (set_color normal)
        case '*'
            printf '%s\n' "$message"
    end
end
