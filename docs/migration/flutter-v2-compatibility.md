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
```

Flutter V2 compatibility still needs:

- same fixture opens through Flutter production services
- Rust edits the fixture
- Flutter reopens it and reads identical content
- same AppError, Candidate, and Mutation semantics

This milestone is not complete until the Flutter side of the same fixture is
verified.
