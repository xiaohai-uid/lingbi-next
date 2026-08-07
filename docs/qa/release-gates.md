# LingBi Release Gates

Status date: 2026-08-07

## Windows is the P0 product platform

The product release is gated exclusively by the Windows Product Gate.
See `.github/workflows/lingbi-next.yml`.

```text
windows-core-gate
```

Runs on `windows-latest` and executes:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd services/cloud
go test ./...
go vet ./...

pnpm install

cd apps/desktop
pnpm test
pnpm build
pnpm tauri build
```

Any step failure prints and fails with:

```text
Windows Product Gate = FAIL
```

## Linux is portability, not approval

```text
linux-portability
```

Linux only verifies:

```text
Rust core compile
Rust unit tests
Go tests
Web build
```

Linux can block a cross-platform Core merge, but:

```text
Linux PASS
≠ Windows Product PASS
```

A green Linux job never authorizes a product release, a Public Beta claim,
or a Windows-specific QA statement. Windows evidence must come from
Windows jobs on Windows runners.

## Installer targets

`apps/desktop/src-tauri/tauri.conf.json` bundles NSIS only:

- per-user install, no administrator rights required (consumer path)
- NSIS is downloaded automatically by `tauri build`; no WiX toolset needed
  on CI
- MSI/WiX can be added later for enterprise distribution, as a separate job

## Evidence taxonomy

QA evidence is strictly labeled, never inferred across platforms:

```text
UNIT                  Rust/Go/TS unit tests
INTEGRATION           Rust/Go/TS integration tests
WINDOWS_CI            Windows runner CI jobs
WINDOWS_DESKTOP_E2E   real Tauri binary driven by tauri-driver on Windows
WINDOWS_CLEAN_MACHINE clean Windows 11 VM / self-hosted runner acceptance
MACOS_COMPATIBILITY   macOS build + unit + desktop smoke
LINUX_PORTABILITY     Linux portability job
LIVE_PROVIDER         real provider acceptance
```

Forbidden inferences:

```text
Linux E2E PASS        → claim Windows E2E PASS
ignored test          → claim CI protected
skipped test          → count as PASS
```
