# LingBi Next

LingBi Next is the local-first desktop/mobile architecture for LingBi.

- Desktop: Rust Core + Tauri 2 + React/TypeScript
- Mobile: existing Flutter shell, progressively bridged to Rust Core
- Cloud: modular Go monolith for account, entitlement, releases, and future
  official AI services
- Website: Next.js for download, account, pricing, and purchase flows

This repository is developed separately from `xiaohai-uid/lingbi` and must not
replace the existing Flutter repository.
