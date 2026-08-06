# Flutter V1 Migration

This directory contains the non-destructive migration contract and fixtures
for existing LingBi Flutter V1 projects.

Rules:

- V1 source is never deleted or rewritten.
- V2 output is always a separate directory.
- Every migrated manuscript file is hashed.
- V2 output must reopen through `ProjectApplicationService`.
