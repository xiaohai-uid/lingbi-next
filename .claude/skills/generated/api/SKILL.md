---
name: api
description: "Skill for the Api area of lingbi-next. 18 symbols across 2 files."
---

# Api

18 symbols | 2 files | Cohesion: 98%

## When to Use

- Working with code in `crates/`
- Understanding how open_project, project_v2_schema_version, list_documents work
- Modifying api-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/lingbi-ffi/src/api/project.rs` | from, open_project, project_v2_schema_version, list_documents, read_document (+5) |
| `crates/lingbi-ffi/src/frb_generated.rs` | wire__crate__api__project__create_document_impl, wire__crate__api__project__list_documents_impl, wire__crate__api__project__open_project_impl, wire__crate__api__project__project_v2_schema_version_impl, wire__crate__api__project__read_document_impl (+3) |

## Entry Points

Start here when exploring this area:

- **`open_project`** (Function) — `crates/lingbi-ffi/src/api/project.rs:78`
- **`project_v2_schema_version`** (Function) — `crates/lingbi-ffi/src/api/project.rs:91`
- **`list_documents`** (Function) — `crates/lingbi-ffi/src/api/project.rs:95`
- **`read_document`** (Function) — `crates/lingbi-ffi/src/api/project.rs:102`
- **`create_document`** (Function) — `crates/lingbi-ffi/src/api/project.rs:110`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `open_project` | Function | `crates/lingbi-ffi/src/api/project.rs` | 78 |
| `project_v2_schema_version` | Function | `crates/lingbi-ffi/src/api/project.rs` | 91 |
| `list_documents` | Function | `crates/lingbi-ffi/src/api/project.rs` | 95 |
| `read_document` | Function | `crates/lingbi-ffi/src/api/project.rs` | 102 |
| `create_document` | Function | `crates/lingbi-ffi/src/api/project.rs` | 110 |
| `save_document` | Function | `crates/lingbi-ffi/src/api/project.rs` | 124 |
| `from` | Function | `crates/lingbi-ffi/src/api/project.rs` | 16 |
| `parse_uuid` | Function | `crates/lingbi-ffi/src/api/project.rs` | 138 |
| `open_project_returns_typed_rust_session` | Function | `crates/lingbi-ffi/src/api/project.rs` | 153 |
| `document_storage_round_trip_uses_typed_rust_api` | Function | `crates/lingbi-ffi/src/api/project.rs` | 177 |
| `wire__crate__api__project__create_document_impl` | Function | `crates/lingbi-ffi/src/frb_generated.rs` | 48 |
| `wire__crate__api__project__list_documents_impl` | Function | `crates/lingbi-ffi/src/frb_generated.rs` | 93 |
| `wire__crate__api__project__open_project_impl` | Function | `crates/lingbi-ffi/src/frb_generated.rs` | 129 |
| `wire__crate__api__project__project_v2_schema_version_impl` | Function | `crates/lingbi-ffi/src/frb_generated.rs` | 165 |
| `wire__crate__api__project__read_document_impl` | Function | `crates/lingbi-ffi/src/frb_generated.rs` | 198 |
| `wire__crate__api__project__save_document_impl` | Function | `crates/lingbi-ffi/src/frb_generated.rs` | 236 |
| `sse_decode` | Function | `crates/lingbi-ffi/src/frb_generated.rs` | 286 |
| `pde_ffi_dispatcher_primary_impl` | Function | `crates/lingbi-ffi/src/frb_generated.rs` | 435 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Pde_ffi_dispatcher_primary_impl → RustAppError` | intra_community | 4 |
| `Pde_ffi_dispatcher_primary_impl → RustDocument` | intra_community | 4 |
| `Pde_ffi_dispatcher_primary_impl → RustProject` | intra_community | 4 |
| `Pde_ffi_dispatcher_primary_impl → From` | intra_community | 4 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Tests | 1 calls |

## How to Explore

1. `context({name: "open_project"})` — see callers and callees
2. `query({search_query: "api"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
