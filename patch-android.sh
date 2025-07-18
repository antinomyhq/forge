#!/bin/bash
# This script will be removed once those android patches merge into upstream.
# If you want local build termux forge, you need apply the changes.

echo "Applying Android patches to Cargo.html for target: $TARGET..."

cat <<EOF>> Cargo.toml

[patch.crates-io]
html2md = {git = "https://gitlab.com/Kanedias/html2md"}
machineid-rs = {git = "https://github.com/shawn111/machineid-rs", branch = "termux"}
EOF

