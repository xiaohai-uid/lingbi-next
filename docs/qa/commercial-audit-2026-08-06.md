# LingBi Commercial Readiness Audit

Date: 2026-08-06

## Classification

```text
Internal Alpha
```

Changed 2026-08-07 (Task 23): the previous "Public Beta / Known failures:
NONE" framing was removed because the Windows Product Gate is not yet
complete. Until then the product is Internal Alpha; see
docs/qa/release-gates.md and docs/release/release-order.md.


Not Commercial GA. The following external gates are not yet satisfied:

- legitimate billing provider and production payment acceptance
- Windows code-signing certificate and signed installer
- signed updater end-to-end validation
- legal/privacy final review
- clean-machine install/upgrade/uninstall/backup restore acceptance
- real provider acceptance

## Completed evidence

- Flutter Milestone 0 P0 stabilization: PASS
- Rust Core through Milestone 23/25 core work: PASS
- REAL Desktop Golden Path E2E through production services: PASS
- Offline behavior E2E: PASS
- Go Cloud health, auth, entitlement, release, billing, checkout endpoints: PASS
- Website required pages and route contract: PASS
- Privacy baseline allow/deny policy: PASS
- Signing policy documented: PASS
- Failing-fast CI release gate: added
- Flutter V2 shared fixture and Rust-edit/Flutter-reopen proof: PASS

## Remaining implementation work

- PDF export with verified Chinese-capable font
- DOCX import
- Full Tauri updater integration and update manifest signature flow
- Website purchase flow against a real billing provider
- Rust Core into Flutter migration order (project parsing and document storage wired; mutation onward remains)
- Advanced feature gates after Desktop Golden Path

## Known failures

```text
UNKNOWN until the Windows Product Gate completes
```

No "NONE" claim may be made while Windows CI / Windows desktop E2E /
clean-machine acceptance are not all green. Evidence is strictly labeled
UNIT / INTEGRATION / WINDOWS_CI / WINDOWS_DESKTOP_E2E /
WINDOWS_CLEAN_MACHINE / MACOS_COMPATIBILITY / LINUX_PORTABILITY /
LIVE_PROVIDER (docs/qa/release-gates.md).


## External blockers

```text
BLOCKED_EXTERNAL:
- Windows code-signing certificate
- legitimate billing merchant account
- legal and privacy final review
- real clean-machine acceptance
- real provider acceptance
```

## Gate

```text
BLOCKED_EXTERNAL for Commercial GA
```
