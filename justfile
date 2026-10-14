# ==========================================================================
# FreeShell - Tauri2 + React SSH client
# Task runner: https://github.com/casey/just
# ==========================================================================

set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# Show available recipes
default:
    @just --list

# --------------------------------------------------------------------------
# Setup
# --------------------------------------------------------------------------

# Install frontend dependencies
install:
    npm install

# Fetch / update rust dependencies
fetch:
    cargo fetch --manifest-path src-tauri/Cargo.toml

# Full setup (frontend deps + rust deps)
setup: install fetch

# --------------------------------------------------------------------------
# Development
# --------------------------------------------------------------------------

# Run the full desktop app in dev mode (vite + tauri, hot reload)
dev:
    npm run tauri dev

# Run only the web frontend (browser, no native shell)
dev-web:
    npm run dev

# Type-check the frontend
typecheck:
    npm run typecheck

# --------------------------------------------------------------------------
# Build
# --------------------------------------------------------------------------

# Build a production desktop bundle
build:
    npm run tauri build

# Build a debug binary with the frontend embedded (no dev server needed)
build-debug:
    npm run build
    cargo build --manifest-path src-tauri/Cargo.toml --features custom-protocol

# Build only the frontend assets into dist/
build-web:
    npm run build

# --------------------------------------------------------------------------
# Quality
# --------------------------------------------------------------------------

# Rust compile check (fast)
check:
    cargo check --manifest-path src-tauri/Cargo.toml --all-targets

# Run the rust test-suite (includes the in-process SSH end-to-end tests)
test:
    cargo test --manifest-path src-tauri/Cargo.toml

# Rust clippy lints
lint:
    cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings

# Format rust sources
fmt:
    cargo fmt --manifest-path src-tauri/Cargo.toml

# Verify rust sources are formatted
fmt-check:
    cargo fmt --manifest-path src-tauri/Cargo.toml --check

# Everything CI would run
verify: typecheck check fmt-check lint test
    @echo "all checks passed"

# Drive the real app over CDP against examples/fake_ssh_server (Windows only)
[windows]
e2e-ui: build-debug
    cargo build --manifest-path src-tauri/Cargo.toml --example fake_ssh_server
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/ui_e2e.ps1

# --------------------------------------------------------------------------
# Housekeeping
# --------------------------------------------------------------------------

# Remove build artifacts
[unix]
clean:
    cargo clean --manifest-path src-tauri/Cargo.toml
    rm -rf dist node_modules

# Remove build artifacts
[windows]
clean:
    cargo clean --manifest-path src-tauri/Cargo.toml
    Remove-Item -Recurse -Force dist, node_modules -ErrorAction SilentlyContinue

# Generate application icons from assets/logo.png
icon:
    npm run tauri icon assets/logo.png
