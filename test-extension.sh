#!/bin/bash
set -e

echo "🚀 ForgeCode VSCode Extension - Local Testing Setup"
echo "=================================================="
echo ""

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Step 1: Build the Rust server
echo -e "${BLUE}📦 Step 1: Building forge-app-server...${NC}"
cargo build --bin forge-app-server
echo -e "${GREEN}✅ Server built successfully${NC}"
echo ""

# Step 2: Verify binary
echo -e "${BLUE}🔍 Step 2: Verifying binary...${NC}"
if [ -f "target/debug/forge-app-server" ]; then
    SIZE=$(ls -lh target/debug/forge-app-server | awk '{print $5}')
    echo -e "${GREEN}✅ Binary exists: target/debug/forge-app-server ($SIZE)${NC}"
else
    echo -e "${YELLOW}⚠️  Binary not found${NC}"
    exit 1
fi
echo ""

# Step 3: Install extension dependencies
echo -e "${BLUE}📚 Step 3: Installing extension dependencies...${NC}"
cd vscode-extension
if [ ! -d "node_modules" ]; then
    npm install
else
    echo "Dependencies already installed"
fi
echo -e "${GREEN}✅ Dependencies ready${NC}"
echo ""

# Step 4: Compile TypeScript
echo -e "${BLUE}🔧 Step 4: Compiling TypeScript...${NC}"
npm run compile
echo -e "${GREEN}✅ TypeScript compiled${NC}"
echo ""

# Step 5: Summary
echo -e "${GREEN}=================================================="
echo "✅ Setup Complete! Ready to test the extension"
echo "==================================================${NC}"
echo ""
echo -e "${YELLOW}Next Steps:${NC}"
echo "1. Open VSCode: code vscode-extension/"
echo "2. Press F5 to start debugging"
echo "3. A new VSCode window will open (Extension Development Host)"
echo "4. Test the features!"
echo ""
echo -e "${YELLOW}Quick Tests:${NC}"
echo "• Cmd+Shift+P → 'ForgeCode: List Agents'"
echo "• Cmd+Shift+P → 'ForgeCode: Show Chat Panel'"
echo "• Check sidebar for 'ForgeCode' views"
echo "• Status bar should show agent and model"
echo ""
echo -e "${YELLOW}Configuration:${NC}"
echo "Server path: $(pwd)/../target/debug/forge-app-server"
echo "Log level: debug (see Output → ForgeCode)"
echo ""
