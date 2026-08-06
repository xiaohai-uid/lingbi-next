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
```

## Verification

```text
cargo test --workspace
all tests pass

cargo clippy --workspace --all-targets -- -D warnings
0 warnings

cd apps/desktop && pnpm test
3 passed

cd apps/desktop && pnpm build
PASS

cd apps/desktop && pnpm tauri build --no-bundle
PASS
```

## Flutter Milestone 0 verification

```text
flutter analyze lib/
No issues found

flutter test --exclude-tags network --concurrency=1
1523 passed
0 failed

flutter build windows --release
PASS

flutter test integration_test/path2_windows_smoke_test.dart -d windows
3 passed
0 failed
```

## Next

Continue with Tauri shell, Tauri permission boundary, Desktop UI Golden Path,
and REAL Desktop E2E before beginning Go Cloud, Website, and Commerce work.
