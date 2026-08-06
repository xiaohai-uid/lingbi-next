# Desktop Golden Path E2E

This crate runs the REAL Desktop Golden Path through production Rust services:

1. Create a project.
2. Assert the first chapter exists.
3. Generate a candidate through `GenerationService` with a fake provider
   injected into the same provider interface used by production.
4. Adopt the candidate through `MutationEngine` and `DocumentApplicationService`.
5. Reopen the project and verify the manuscript content survived restart.

Run:

```bash
cargo test -p lingbi-e2e-desktop
```

The test does not manually create a candidate. It only asserts the candidate
returned by the production generation path.
