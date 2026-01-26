#!/bin/bash
# Script to install Rust Clippy
# Clippy requires rustup to be installed

echo "To install Rust Clippy, you need rustup."
echo ""
echo "Install rustup with one of these methods:"
echo ""
echo "1. Using snap (recommended for Ubuntu/Debian):"
echo "   sudo snap install rustup"
echo ""
echo "2. Using the official installer:"
echo "   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
echo ""
echo "After installing rustup, run:"
echo "   rustup component add clippy"
echo ""
echo "Then you can use clippy with:"
echo "   cargo clippy"
