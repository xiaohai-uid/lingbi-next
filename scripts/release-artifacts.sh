#!/usr/bin/env bash
# P13 #56 发布物管线：NSIS + 便携 ZIP + SHA256SUMS + PROVENANCE。
#
# 用法：scripts/release-artifacts.sh [version]
#   version 缺省取 tauri.conf.json 的 version。
# 产物落 apps/desktop/src-tauri/target/release/bundle/release/
#   - LingBi Next_<v>_x64-setup.exe  （NSIS 安装包）
#   - LingBi Next_<v>_x64-portable.zip（便携版）
#   - SHA256SUMS / PROVENANCE
# 未签名阶段：PROVENANCE 如实声明 unsigned + 信任锚 = 校验和与构建来源。
set -u
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" || exit 1

VERSION="${1:-$(python3 -c "import json;print(json.load(open('apps/desktop/src-tauri/tauri.conf.json'))['version'])")}"
ROOT="$(pwd)"
OUT="$ROOT/apps/desktop/src-tauri/target/release/bundle/release"
mkdir -p "$OUT"

echo "== 构建 $VERSION（NSIS）=="
(cd apps/desktop && pnpm tauri build) || exit 1

echo "== 收集产物 =="
NSIS="$ROOT/apps/desktop/src-tauri/target/release/bundle/nsis/LingBi Next_${VERSION}_x64-setup.exe"
EXE="$ROOT/apps/desktop/src-tauri/target/release/lingbi-desktop.exe"
ZIP_DST="$OUT/LingBi Next_${VERSION}_x64-portable.zip"

[ -f "$NSIS" ] || { echo "缺 NSIS 产物: $NSIS"; exit 1; }
[ -f "$EXE" ] || { echo "缺 release exe: $EXE"; exit 1; }
cp "$NSIS" "$OUT/"
# 便携版 = release exe 自包含（Tauri v2 Windows exe 单文件；WebView2 系统
# 自带）。Tauri 无 zip bundle 目标，手工组装（python zipfile——Git Bash 无 zip）。
rm -f "$ZIP_DST"
python3 -c "import zipfile,sys; z=zipfile.ZipFile(sys.argv[1],'w',zipfile.ZIP_DEFLATED); z.write(sys.argv[2],'lingbi-desktop.exe'); z.close()" \
    "$ZIP_DST" "$EXE" || exit 1

echo "== SHA256SUMS =="
cd "$OUT"
sha256sum "LingBi Next_${VERSION}_x64-setup.exe" "LingBi Next_${VERSION}_x64-portable.zip" > SHA256SUMS
cat SHA256SUMS

echo "== PROVENANCE =="
{
    echo "product: LingBi Next"
    echo "version: $VERSION"
    echo "release_phase: Internal Alpha / Closed Beta / Preview（未签名先行）"
    echo "signed: false"
    echo "trust_anchor: SHA256SUMS + 构建来源声明（1.0.0 起 Authenticode 签名）"
    echo "source_commit: $(git rev-parse HEAD 2>/dev/null || echo unknown)"
    echo "built_at: $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    echo "builder: $(uname -s) $(uname -m)"
    echo "gate_summary: $(grep -h 'PASS=' /tmp/release-gate-summary.txt 2>/dev/null || echo 'run scripts/release-gate.sh')"
} > PROVENANCE
cat PROVENANCE

echo
echo "== 发布物清单（$OUT）=="
ls -la "$OUT"
