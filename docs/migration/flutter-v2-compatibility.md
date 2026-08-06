# Flutter V2 Compatibility

The shared Project V2 fixture is at:

```text
fixtures/projects/project-v2
```

Rust production services open this fixture and verify:

- project metadata uses `schema_version: 2`
- document metadata is stored in `.lingbi/documents.json`
- manuscript files are UUID-backed under `chapters/<document-uuid>.md`
- content hash matches the canonical file bytes

Run:

```bash
cargo test -p lingbi-application --test project_v2_fixture
flutter test test/project_v2_compatibility_test.dart
```

The shared fixture compatibility is verified end to end:

- Rust production services open the fixture
- Flutter production services open the fixture
- Rust edits a copy through `DocumentApplicationService`
- Flutter reopens that edited copy and reads the same revision, hash, and content

Run the real cross-platform proof:

```bash
scripts/cross-platform-v2-proof.sh
```

Rust Core `AppError`, Candidate, and Mutation semantics into Flutter remain
Milestone 30 work.

## Flutter-Rust bridge

Milestone 30 now exposes Project V2 parsing through flutter_rust_bridge:

- `crates/lingbi-ffi` defines `RustProjectSession`, `RustProject`,
  `RustDocument`, and `RustAppError`
- generated Dart bindings live in the Flutter app under `lib/src/rust`
- `RustCore` in Flutter initializes the bridge and calls `openProject`
- Cargokit bundles `lingbi_ffi.dll` into the Windows release build
- document storage is exposed through `listDocuments`, `readDocument`,
  `createDocument`, and `saveDocument`

Run:

```bash
scripts/flutter-rust-bridge-proof.sh
```

The remaining Milestone 30 order is mutation, recovery, AI provider,
generation, and import/export.
