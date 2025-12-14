#!/usr/bin/env bash

# Performance test script for forge info command
# Runs the command 10 times and collects timing statistics

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
COMMAND="target/debug/forge info"
ITERATIONS=10
TIMES=()

# Header
echo ""
echo -e "🚀 ${BOLD}Performance Test${RESET} ${DIM}—${RESET} ${CYAN}forge info${RESET}"
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

# Results summary
echo -e "📊 ${BOLD}Summary${RESET}"
echo ""
printf "  ${DIM}avg${RESET}  ${CYAN}%5d${RESET} ${DIM}ms${RESET}\n" $AVG
printf "  ${DIM}min${RESET}  ${GREEN}%5d${RESET} ${DIM}ms${RESET}\n" $MIN
printf "  ${DIM}max${RESET}  ${YELLOW}%5d${RESET} ${DIM}ms${RESET}\n" $MAX
echo ""
