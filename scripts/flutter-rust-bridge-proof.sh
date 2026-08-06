#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
flutter_root="${LINGBI_FLUTTER_ROOT:-/mnt/c/codex/worktrees/lingbi-stabilize-p0}"
repo_windows="$(wslpath -w "$repo_root")"
flutter_windows="$(wslpath -w "$flutter_root")"
rust_crate_windows="$(wslpath -w "$repo_root/crates/lingbi-ffi")"
dll_path="$flutter_root/build/windows/x64/plugins/lingbi_ffi/Release/lingbi_ffi.dll"

cmd.exe /d /s /c "cd /d $flutter_windows && set LINGBI_RUST_CRATE_DIR=$rust_crate_windows&& C:\\Flutter\\flutter\\bin\\flutter.bat build windows --release"

windows_dll="$(wslpath -m "$dll_path")"
cmd.exe /d /s /c "cd /d $flutter_windows && C:\\Flutter\\flutter\\bin\\flutter.bat test test/rust_ffi_project_parser_test.dart test/rust_ffi_document_storage_test.dart --dart-define=LINGBI_FFI_DLL=$windows_dll --concurrency=1"
