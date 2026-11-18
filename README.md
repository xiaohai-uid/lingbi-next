# LingBi Next

LingBi Next is the local-first desktop architecture for LingBi — an AI-assisted novel writing tool.

- **Desktop**: Rust Core + Tauri 2 + React/TypeScript, Apple (macOS) design language
- **Mobile**: existing Flutter shell, progressively bridged to Rust Core
- **Cloud**: modular Go monolith for account, entitlement, releases, and future official AI services
- **Website**: Next.js for download, account, pricing, and purchase flows

For the Windows writing app, see [LingBi releases](https://github.com/xiaohai-uid/lingbi/releases). To work on the Rust/Tauri implementation in this repository, start with the build and test commands below.

## Status: 0.9.0 (Fusion P0–P14 complete)

All fusion phases shipped on `main`:

| Phase | Deliverable |
|-------|-------------|
| P0–P3 | Book V3 contract, golden corpus, Rust Book Core |
| P4 | Command layer (16 commands, Windows-tested) |
| P5 | Product layer rebuild (capability audit + vertical slices) |
| P6 | V1 → Book V3 migration (byte-for-byte, 零界道种) |
| P7 | Chapter transaction engine (8-node chain, input_hash) |
| P8 | Deterministic workflow (Run persistence, TokenLedger, OutputGate) |
| P9 | Product UI — golden path always holds; 4-step mode default |
| P10 | Git optional (in-app snapshot) |
| P11 | Consumer E2E — full journey, restart zero-loss |
| P12 | Real-book product acceptance (零界道种) |
| P13 | 0.9.0 release pipeline (NSIS / ZIP / SHA256SUMS / PROVENANCE) |
| P14 | 1.0.0 gate checklist + archive prep |

## Architecture

- **Rust Core** (`crates/`): workflow engine, chapter transactions, import/export, migration
- **Tauri desktop** (`apps/desktop`): React frontend, CodeMirror editor, Apple design system
- **Contracts** (`contracts/`): Book V3 schema + tree contracts, validated by `scripts/validate-book-v3.py`
- **E2E** (`e2e/`): unit-level command paths + real Windows desktop E2E (product gate)

## Build

```bash
pnpm install
cd apps/desktop
pnpm tauri build   # produces NSIS installer + portable ZIP
```

## Test

```bash
pnpm --filter @lingbi/desktop test  # frontend Vitest suite
cargo test --workspace # Rust core
```

Real Windows desktop E2E requires a release binary + Edge WebDriver (opt-in CI gate).

## Repository layout

This repository is developed separately from `xiaohai-uid/lingbi` and must not
replace the existing Flutter repository.
