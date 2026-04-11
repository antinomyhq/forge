# Find the 1-based index of a value in porcelain output for fzf positioning
# Expects porcelain output WITH headers and skips the header line.
# Returns the index if found, 1 otherwise.
# Usage: _forge_find_index <output> <value_to_find> [field_number] [field_number2] [value_to_find2]
#   field_number: which porcelain column to compare (1-based, using multi-space delimiter)
#   field_number2/value_to_find2: optional second column+value for compound matching

function _forge_find_index
    set -l output $argv[1]
    set -l value_to_find $argv[2]
    set -l field_number 1
    set -l field_number2 ""
    set -l value_to_find2 ""

    if test (count $argv) -ge 3
        set field_number $argv[3]
    end
    if test (count $argv) -ge 4
        set field_number2 $argv[4]
    end
    if test (count $argv) -ge 5
        set value_to_find2 $argv[5]
    end

    set -l index 1
    set -l line_num 0

    # Process each line of the output
    for line in (string split \n -- "$output")
        set line_num (math $line_num + 1)

        # Skip the header line (first line)
        if test $line_num -eq 1
            continue
        end

        # Extract field value using awk with multi-space delimiter
        set -l field_value (echo "$line" | awk -F '  +' "{print \$$field_number}")

        if test "$field_value" = "$value_to_find"
            if test -n "$field_number2"; and test -n "$value_to_find2"
                set -l field_value2 (echo "$line" | awk -F '  +' "{print \$$field_number2}")
                if test "$field_value2" = "$value_to_find2"
                    echo "$index"
                    return 0
                end
            else
                echo "$index"
                return 0
            end
        end

        set index (math $index + 1)
    end

    echo "1"
    return 0
end
