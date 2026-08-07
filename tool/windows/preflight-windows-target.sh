#!/usr/bin/env bash
# Windows-target preflight for lingbi-next (run inside WSL before pushing).
#
# The Linux-only validation never compiles #[cfg(windows)] branches — the
# 2026-08-07 regression proved that (unstable PermissionsExt + a missed
# collapsible_if shipped because Linux CI was green). This script adds the
# missing axis: cross-check and cross-clippy against x86_64-pc-windows-gnu.
#
# lingbi-ffi is excluded on purpose: its Windows build is produced by the
# Flutter/cargokit toolchain on real Windows (ffi-gate job); dart-sys needs
# the mingw toolchain, which WSL here does not have.
#
# Usage:
#   tool/windows/preflight-windows-target.sh
#
# Exit 0 = Windows-target compile + clippy clean. Any error = do NOT push.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if ! rustup target list --installed | grep -q x86_64-pc-windows-gnu; then
  echo "installing x86_64-pc-windows-gnu std (one-time)..."
  rustup target add x86_64-pc-windows-gnu
fi

echo "==> cargo check (workspace, windows target)"
cargo check --workspace --exclude lingbi-ffi --target x86_64-pc-windows-gnu

echo "==> cargo clippy (workspace, windows target, -D warnings)"
cargo clippy --workspace --exclude lingbi-ffi --all-targets --target x86_64-pc-windows-gnu -- -D warnings

echo "==> cargo clippy (e2e crates, windows target, -D warnings)"
cargo clippy -p lingbi-e2e-desktop-real -p lingbi-e2e-desktop --all-targets --target x86_64-pc-windows-gnu -- -D warnings

echo "==> desktop shell (linux target)"
cd apps/desktop/src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings

echo
echo "Windows-target preflight: PASS"
