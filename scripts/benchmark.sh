#!/usr/bin/env bash

# Performance test script for forge commands
# Runs the command 10 times and collects timing statistics
# Usage: ./benchmark.sh [--threshold MS] [command args...]
# Example: ./benchmark.sh info
# Example: ./benchmark.sh --threshold 50 zsh rprompt
# Example: ./benchmark.sh --version

set -euo pipefail

# Colors and styling
BOLD='\033[1m'
DIM='\033[2m'
RESET='\033[0m'
GREEN='\033[32m'
YELLOW='\033[33m'
CYAN='\033[36m'
GRAY='\033[90m'

# Configuration
BASE_COMMAND="target/debug/forge"
THRESHOLD=""

# Parse arguments
ARGS=()
while [[ $# -gt 0 ]]; do
    case $1 in
        --threshold)
            THRESHOLD="$2"
            shift 2
            ;;
        *)
            ARGS+=("$1")
            shift
            ;;
    esac
done

# Build command
if [ ${#ARGS[@]} -gt 0 ]; then
    COMMAND="$BASE_COMMAND ${ARGS[*]}"
else
    COMMAND="$BASE_COMMAND"
fi
ITERATIONS=10
TIMES=()

# Extract command name for display
DISPLAY_CMD=$(echo "$COMMAND" | sed "s|target/debug/||")

# Header
echo ""
echo -e "🚀 ${BOLD}Performance Test${RESET} ${DIM}—${RESET} ${CYAN}${DISPLAY_CMD}${RESET}"
echo ""

# Build step
echo -e "${GRAY}📦 Building...${RESET}"
cargo build 2>&1 | grep -E "Compiling|Finished" | tail -1
echo ""

# Show sample output
echo -e "${GRAY}📋 Sample output:${RESET}"
echo ""
$COMMAND
echo ""

# Run performance tests
echo -e "${GRAY}⏱️  Running ${YELLOW}$ITERATIONS${GRAY} iterations...${RESET}"
echo ""

for i in $(seq 1 $ITERATIONS); do
    # Measure execution time
    START=$(date +%s%N)
    $COMMAND > /dev/null 2>&1
    END=$(date +%s%N)
    
    # Calculate duration in milliseconds
    DURATION=$(( (END - START) / 1000000 ))
    TIMES+=($DURATION)
    
    # Color code based on performance
    if [ $DURATION -lt 50 ]; then
        COLOR=$GREEN
    elif [ $DURATION -lt 100 ]; then
        COLOR=$YELLOW
    else
        COLOR=$GRAY
    fi
    
    printf "  ${DIM}%2d${RESET}  ${COLOR}%5d${RESET} ${DIM}ms${RESET}\n" $i $DURATION
done

echo ""

# Calculate statistics
TOTAL=0
MIN=${TIMES[0]}
MAX=${TIMES[0]}

for time in "${TIMES[@]}"; do
    TOTAL=$((TOTAL + time))
    if [ $time -lt $MIN ]; then
        MIN=$time
    fi
    if [ $time -gt $MAX ]; then
        MAX=$time
    fi
done

AVG=$((TOTAL / ITERATIONS))

# Calculate median (P50): sort the array and pick the middle value
# Using sort -n and awk to get the median of ITERATIONS values
SORTED=($(printf '%d\n' "${TIMES[@]}" | sort -n))
HALF=$((ITERATIONS / 2))
if (( ITERATIONS % 2 == 0 )); then
    MEDIAN=$(( (SORTED[HALF - 1] + SORTED[HALF]) / 2 ))
else
    MEDIAN=${SORTED[HALF]}
fi

# Results summary
echo -e "📊 ${BOLD}Summary${RESET}"
echo ""
printf "  ${DIM}avg${RESET}    ${CYAN}%5d${RESET} ${DIM}ms${RESET}\n" $AVG
printf "  ${DIM}median${RESET} ${CYAN}%5d${RESET} ${DIM}ms${RESET}\n" $MEDIAN
printf "  ${DIM}min${RESET}    ${GREEN}%5d${RESET} ${DIM}ms${RESET}\n" $MIN
printf "  ${DIM}max${RESET}    ${YELLOW}%5d${RESET} ${DIM}ms${RESET}\n" $MAX
echo ""

# Check threshold if provided
# Use median instead of mean to reduce sensitivity to outliers caused by
# CI runner variance (e.g., scheduled maintenance, noisy neighbors).
if [ -n "$THRESHOLD" ]; then
    if [ $MEDIAN -gt $THRESHOLD ]; then
        echo -e "❌ ${BOLD}Performance regression detected!${RESET}"
        echo -e "   Median time ${CYAN}${MEDIAN}ms${RESET} exceeds threshold ${YELLOW}${THRESHOLD}ms${RESET}"
        echo -e "   ${DIM}(avg: ${AVG}ms, min: ${MIN}ms, max: ${MAX}ms)${RESET}"
        echo ""
        exit 1
    else
        echo -e "✅ ${BOLD}Performance check passed!${RESET}"
        echo -e "   Median time ${CYAN}${MEDIAN}ms${RESET} is within threshold ${YELLOW}${THRESHOLD}ms${RESET}"
        echo -e "   ${DIM}(avg: ${AVG}ms, min: ${MIN}ms, max: ${MAX}ms)${RESET}"
        echo ""
    fi
fi
