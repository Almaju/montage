#!/bin/bash
# Setup script for Montage development

set -e

echo "🔧 Setting up Montage development environment..."

# Configure git hooks
git config core.hooksPath .githooks
echo "✅ Git hooks configured"

# Install clippy if not present
if ! rustup component list | grep -q "clippy.*installed"; then
    echo "📦 Installing clippy..."
    rustup component add clippy
fi
echo "✅ Clippy available"

echo ""
echo "🎬 Setup complete! You can now run:"
echo "   cargo build    - Build the project"
echo "   cargo run      - Run the app"
echo "   cargo clippy   - Run lints"
