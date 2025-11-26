#!/bin/bash

# Backup script with error handling
set -euo pipefail

# Configuration
BACKUP_DIR="/backup"
SOURCE_DIR="/data"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
LOG_FILE="/var/log/backup.log"

# Function to log messages
log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

# Function to handle errors
handle_error() {
    log "ERROR: Backup failed on line $1"
    cleanup
    exit 1
}

# Cleanup function
cleanup() {
    log "Performing cleanup..."
    # Remove any temporary files
}

# Set error trap
trap 'handle_error $LINENO' ERR

# Main backup logic
log "Starting backup process"

if [[ ! -d "$SOURCE_DIR" ]]; then
    log "ERROR: Source directory $SOURCE_DIR does not exist"
    exit 1
fi

if [[ ! -d "$BACKUP_DIR" ]]; then
    log "Creating backup directory $BACKUP_DIR"
    mkdir -p "$BACKUP_DIR"
fi

BACKUP_FILE="$BACKUP_DIR/backup_$TIMESTAMP.tar.gz"

log "Creating backup: $BACKUP_FILE"
tar -czf "$BACKUP_FILE" -C "$(dirname "$SOURCE_DIR")" "$(basename "$SOURCE_DIR")"

if [[ $? -eq 0 ]]; then
    log "Backup completed successfully"
    log "Backup size: $(du -h "$BACKUP_FILE" | cut -f1)"
else
    log "ERROR: Backup failed"
    exit 1
fi

log "Backup process completed"