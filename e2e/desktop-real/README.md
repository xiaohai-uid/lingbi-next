# Real Desktop Binary E2E (`lingbi-e2e-desktop-real`)

Drives the **real LingBi Tauri release binary** through `tauri-driver`
(WebDriver protocol), exercising the real React UI, real IPC, and real
Rust Core. This crate intentionally depends only on the binary it drives —
no lingbi crates — so it can never accidentally test in-process code.

## Platforms

| Platform | Role | Driver | Missing infra |
|---|---|---|---|
| Windows | **Product E2E** | tauri-driver + Edge WebDriver (WebView2) | hard FAIL |
| Linux | compatibility only | tauri-driver + WebKitWebDriver (Xvfb) | explicit compat-skip, never a pass |

No user-home paths are hardcoded. Driver resolution order:

1. `LINGBI_EDGE_DRIVER` (Windows) / `LINGBI_WEBKIT_DRIVER` (Linux) env var
2. well-known machine location (GitHub Actions Windows: Edge WebDriver)
3. tauri-driver PATH auto-detection

## Windows product gate

`.github/workflows/lingbi-next.yml` → `windows-desktop-e2e` job:

1. `pnpm tauri build` produces `apps/desktop/src-tauri/target/release/lingbi-desktop.exe`
2. `cargo install tauri-driver --locked`
3. `cargo test -p lingbi-e2e-desktop-real --test windows_desktop_e2e -- --test-threads=1`

Acceptance (enforced in the workflow):

```text
Windows Desktop E2E passed > 0
Windows Desktop E2E failed  = 0
Windows Desktop E2E skipped = 0
```

The Windows test is never `#[ignore]`d and panics when the binary is
missing. The Linux test lives in `tests/linux_compat_e2e.rs` and only
prints a compat-skip when the binary or driver is unavailable.

## Local run (Linux compat, requires built binary)

```bash
pnpm install
cd apps/desktop
pnpm tauri build --no-bundle   # or full build
cargo test -p lingbi-e2e-desktop-real --test linux_compat_e2e -- --test-threads=1
```
