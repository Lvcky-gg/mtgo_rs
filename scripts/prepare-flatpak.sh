#!/usr/bin/env bash
set -euo pipefail

# Vendor on the runner, then compile without network access inside the matching
# SDK. Building a host binary and copying it into a Flatpak can mismatch glibc.
mkdir -p .cargo dist
cargo vendor --locked vendor > .cargo/config.toml
tar --exclude='./.git' --exclude='./target' --exclude='./dist' \
    --exclude='./build-dir' --exclude='./flatpak-repo' --exclude='./.flatpak-builder' \
    -czf dist/flatpak-source.tar.gz .
