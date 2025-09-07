#!/bin/bash

# Pre-push validation script - runs the same checks as GitHub Actions
# This script replicates the workflow in .github/workflows/build.yaml

set -e  # Exit on any error

echo "🔍 Pre-push validation starting..."
echo "Running the same checks as GitHub Actions CI..."
echo ""

# Check if we're in a git repository and have Rust files
if [ ! -f "Cargo.toml" ]; then
    echo "❌ No Cargo.toml found. This script should be run in the Rust project root."
    exit 1
fi


echo "📦 Step 1: Building project (release mode)..."
if cargo build --release --verbose; then
    echo "✅ Build successful"
else
    echo "❌ Build failed"
    exit 1
fi
echo ""

echo "🧪 Step 2: Running tests..."
if cargo test --verbose; then
    echo "✅ Tests passed"
else
    echo "❌ Tests failed"
    exit 1
fi
echo ""

echo "🔍 Step 3: Running clippy (linter)..."
if cargo clippy --all-targets --all-features; then
    echo "✅ Clippy checks passed"
else
    echo "❌ Clippy found issues"
    exit 1
fi
echo ""

echo "📝 Step 4: Checking code formatting..."
if cargo fmt --all --check; then
    echo "✅ Code formatting is correct"
else
    echo "❌ Code formatting issues found"
    echo "💡 Run 'cargo fmt --all' to fix formatting"
    exit 1
fi
echo ""

echo "🎉 All pre-push checks passed!"
echo "✅ Your code is ready to be pushed to the repository"