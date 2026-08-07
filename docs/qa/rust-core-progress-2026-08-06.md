# LingBi Next Rust Core Progress

Date: 2026-08-06

## Completed execution-order items

- Milestone 0: Flutter P0 stabilization
- Milestone 1: LingBi Next Rust workspace
- Milestone 2: Typed AppError and Project V2 contracts
- Milestone 3: Atomic file store and project path guard
- Milestone 4: Single ProjectApplicationService
- Milestone 5: Approval-gated mutation engine
- Milestone 6: Fail-closed recovery
- Milestone 7: Non-destructive Flutter V1 to V2 migration
- Milestone 8: Typed streaming AI provider core
- Milestone 9: Secret store abstraction
- Milestone 10: Recoverable generation state machine
- Milestone 11: Tauri React desktop shell with application commands
- Milestone 12: Deny-by-default Tauri capability boundary
- Milestone 13 (shell): Welcome, project create/open, Markdown editor, save, and candidate panel scaffold
- Milestone 14: REAL Desktop Golden Path E2E through production services
- Milestone 15 (partial): Markdown/TXT/DOCX export and verified portable ZIP exchange
- Milestone 16: Modular Go Cloud bootstrap with health and readiness endpoints
- Milestone 17: Account system with auth endpoints, hashed refresh tokens, and account migration
- Milestone 18: Ed25519 offline entitlement service with verification tests
- Milestone 20: Next.js website scaffold with required public/account pages
- Milestone 21: Go release/download endpoints behind ReleaseStorage
- Milestone 23: Billing abstraction and idempotent webhook path
- Milestone 19: Offline behavior proof for local manuscript editing and graceful AI failure
- Milestone 22: Signed update manifest verification primitive
- Milestone 25: Sandbox checkout and billing webhook endpoints
- Milestone 26: Failing-fast CI gate across Rust, frontend, website, Go, and Tauri
- Milestone 28: Privacy baseline allow/deny policy
- Milestone 27: Signing policy for code, updater, and entitlement trust roots
- Commercial readiness audit: Internal Alpha classification (was Public Beta; reclassified 2026-08-07, Task 23) and external gate list
- Milestone 29: shared Project V2 fixture compatibility, including Rust edit and Flutter reopen proof
- Milestone 30: Project V2 C ABI bridge plus flutter_rust_bridge project parsing and document storage with Cargokit Windows bundling, followed by the 15 local fix tasks on `milestone_30_continue_mutation`

## Repositories

- Existing Flutter: `xiaohai-uid/lingbi`, branch `stabilize/flutter-p0`
- New Rust workspace: `/home/a1691/lingbi-next`, branch `main`

## LingBi Next commits

```text
07dff7c chore: bootstrap LingBi Next Rust workspace
f10b50f feat(contracts): define typed application errors
6876f63 feat(project): define portable project v2 contract
74fac70 feat(storage): add atomic hash-verified file store
24cf621 feat(security): enforce project filesystem boundaries
adf0359 feat(application): establish single project session entry point
446dd15 feat(mutation): add approval-gated manuscript mutation engine
8dee05b feat(recovery): implement fail-closed project recovery
ca508bf feat(migration): add non-destructive Flutter V1 to V2 migration
3f14dc6 feat(ai): implement typed streaming provider core
9d4b05e feat(security): isolate provider secrets from frontend state
db42e75 feat(writing): add recoverable generation state machine
bea6604 feat(application): add revision-safe document CRUD
305cc31 feat(desktop): bootstrap Tauri React shell with application commands
392eb44 feat(desktop): enforce deny-by-default Tauri capabilities
6b4d918 feat(desktop): add Golden Path shell UI tests
2b85ff8 feat(application): add approval-gated generation and candidate adoption
dd67c87 feat(desktop): wire generation and candidate commands
04d0427 test(e2e): add REAL Desktop Golden Path
1f26587 feat(import-export): ship verified portable manuscript exchange
1126ac9 chore(cloud): bootstrap modular Go backend
07ecbf8 feat(cloud): add account system with auth endpoints
e1cc8fa feat(cloud): add Ed25519 offline entitlement service
e1996ca feat(website): add Next.js site pages
5c90671 chore(website): exclude Next.js build output
4c4d4c4 feat(cloud): add release and download endpoints
43a806c feat(cloud): add billing abstraction with idempotent webhooks
b9f3f56 test(e2e): verify offline manuscript remains usable when AI fails
2a618e6 feat(security): add signed update manifest verification
e7e4dab feat(cloud): add sandbox checkout and billing webhook endpoints
34bd816 ci: add LingBi Next release gate pipeline
acffc10 feat(security): add privacy baseline allow/deny policy
324ce20 test(project): add shared Project V2 fixture compatibility
684f7dd feat(ffi): add Project V2 C ABI bridge
c96e80d test(project): add Rust edit helper for cross-platform V2 proof
c298fae feat(ffi): add flutter_rust_bridge Project V2 API
4a8b57e feat(ffi): add document storage bridge API
```

Flutter branch commits:

```text
c23ccc0 fix(test): skip transient live provider failures
092a6b1 test(project): add Rust-to-Flutter V2 reopen proof
46d0724 feat(ffi): wire flutter_rust_bridge Project V2 parsing
204cb6d feat(ffi): expose Rust document storage to Flutter
```

Local fix-task commits (branch `milestone_30_continue_mutation`, ahead of `origin/main`):

```text
40647c0 fix(storage): compare expected hash against disk and verify writes
5098d79 fix(application): pass expected content hash on save
4da9ebc feat(storage): add recoverable document save transactions
217a306 refactor(core): unify candidate schema across services
b4f6321 fix(candidate): reject stale candidates before adoption
0a48bdd refactor(core): route candidate adoption through mutation coordinator
2172a92 feat(mutation): persist intents, approvals, receipts and document revisions
0a90dd6 feat(recovery): recover mutation intents across crash failpoints
1708628 test(candidate): verify ten consecutive adoptions
7947c68 fix(migration): classify chapters and resources by directory
dd00816 feat(desktop): return structured command errors
f061638 feat(security): persist desktop API keys in OS keyring
bdc495c feat(ai): return full provider test contract
013d783 feat(desktop): stream generation events and cancel in flight
1e718d6 feat(desktop): wire multi-chapter list, create, and select
```

## Verification

```text
cargo test --workspace
all tests pass

cargo clippy --workspace --all-targets -- -D warnings
0 warnings

cd apps/desktop && pnpm test
7 passed

cd apps/desktop && pnpm build
PASS

cd apps/desktop && pnpm tauri build --no-bundle
PASS

cargo test -p lingbi-e2e-desktop
1 passed

cargo test -p lingbi-e2e-desktop-real --test linux_compat_e2e
1 passed (compat) / explicit LINUX_COMPAT_SKIP when infra missing

cargo test -p lingbi-e2e-desktop-real --test windows_desktop_e2e
Windows product E2E via tauri-driver + Edge WebDriver; runs in the
windows-desktop-e2e CI job (never #[ignore]d, never skipped)

cargo test --workspace --lib
all tests pass

cd services/cloud && go test ./...
10 passed

cd apps/website && pnpm test
1 passed

cd apps/website && pnpm build
PASS
```

## 2026-08-07 update

The real desktop binary E2E now passes against a Tauri-CLI-built release. It
exercises project create, multi-chapter create/select, edit, save, provider
configure, streaming generation, candidate adoption, and reopen with persisted
chapter state.

Fixes landed on `milestone_30_continue_mutation`:

- `lingbi-security` enables native OS keyring backends (`apple-native`,
  `windows-native`, `linux-native`) so configured provider keys actually persist
  instead of silently using the keyring mock store.
- Desktop streaming listens for `generation-event` before starting generation,
  so fast SSE candidates are no longer lost to a listener-registration race.
- Tauri commands keep `project_get_session` current when documents are created,
  opened, saved, or adopted, so the backend session matches the selected chapter.
- The real desktop E2E uses a TCP-only Xvfb so it no longer depends on a
  writable `/tmp/.X11-unix`, and its assertions match the real command contract.

## Milestone 15 status

Implemented and verified:

- Markdown and TXT import
- Markdown, TXT, and minimal DOCX export
- Portable ZIP package with `MANIFEST.json`
- Unsafe path rejection for `..`, absolute paths, drive prefixes, UNC paths, and NUL
- SHA-256 checksum validation before extraction

Not yet implemented:

- PDF export and Chinese-capable PDF fixture
- DOCX import
- Flutter V1 package import as a separate portable format

PDF remains `BLOCKED_EXTERNAL` until a verified Chinese-capable PDF path exists.

## Flutter Milestone 0 verification

```text
flutter analyze lib/
No issues found

flutter test --exclude-tags network --concurrency=1
1524 passed
0 failed
3 skipped (fixture/FFI acceptance tests without generated artifacts)

flutter build windows --release
PASS

flutter test integration_test/path2_windows_smoke_test.dart -d windows
3 passed
0 failed
```

Cross-platform Project V2 proof:

```text
scripts/cross-platform-v2-proof.sh
PASS
```

flutter_rust_bridge proof:

```text
scripts/flutter-rust-bridge-proof.sh
PASS
```

## Next

Continue Milestone 30: migrate mutation, recovery, AI provider, generation,
and import/export through `lingbi-ffi`/flutter_rust_bridge. After that, finish
PDF export, DOCX import, updater integration, and the external Commercial GA
gates.
