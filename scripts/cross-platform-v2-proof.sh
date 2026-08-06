#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
flutter_root="${LINGBI_FLUTTER_ROOT:-/mnt/c/codex/worktrees/lingbi-stabilize-p0}"
mkdir -p /mnt/c/codex/tmp
fixture_root="$(mktemp -d /mnt/c/codex/tmp/lingbi-v2-cross.XXXXXX)"
trap 'rm -rf "$fixture_root"' EXIT

cp -R "$repo_root/fixtures/projects/project-v2/." "$fixture_root/"
(
  cd "$repo_root"
  cargo run -q -p lingbi-ffi --bin cross_platform_edit -- "$fixture_root"
)

windows_fixture="$(wslpath -m "$fixture_root")"
windows_flutter_root="$(wslpath -w "$flutter_root")"
cmd.exe /d /s /c "cd /d $windows_flutter_root && C:\\Flutter\\flutter\\bin\\flutter.bat test test/project_v2_cross_platform_test.dart --dart-define=LINGBI_V2_CROSS_FIXTURE=$windows_fixture --concurrency=1"
