#!/bin/bash
# Manual extension launch script

echo "🚀 Launching ForgeCode Extension Manually"
echo "========================================="
echo ""

# Get absolute paths
EXTENSION_PATH="/Users/amit/code-forge/vscode-extension"
WORKSPACE_PATH="/Users/amit/code-forge"

echo "Extension path: $EXTENSION_PATH"
echo "Workspace path: $WORKSPACE_PATH"
echo ""

# Check files exist
if [ ! -f "$EXTENSION_PATH/out/extension.js" ]; then
    echo "❌ Error: out/extension.js not found!"
    echo "Run: cd $EXTENSION_PATH && npm run compile"
    exit 1
fi

if [ ! -f "$EXTENSION_PATH/package.json" ]; then
    echo "❌ Error: package.json not found!"
    exit 1
fi

echo "✅ Files exist"
echo ""
echo "🚀 Launching Extension Development Host..."
echo ""

# Launch VSCode with extension development
code \
  --extensionDevelopmentPath="$EXTENSION_PATH" \
  "$WORKSPACE_PATH"

echo ""
echo "✅ Command executed!"
echo ""
echo "📋 What should happen:"
echo "   1. New VSCode window opens"
echo "   2. Title bar shows: [Extension Development Host]"
echo "   3. ForgeCode icon appears in Activity Bar"
echo ""
echo "🔍 If window doesn't open:"
echo "   - VSCode might already be running with that window"
echo "   - Check if a window opened in the background"
echo "   - Try closing all VSCode windows first"
echo ""
