---
name: cluster-4
description: "Skill for the Cluster_4 area of lingbi-next. 22 symbols across 1 files."
---

# Cluster_4

22 symbols | 1 files | Cohesion: 82%

## When to Use

- Working with code in `crates/`
- Understanding how new, create_document, read_document work
- Modifying cluster_4-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/lingbi-application/src/document_service.rs` | new, create_document, read_document, list_documents, get_document (+17) |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `crates/lingbi-application/src/document_service.rs:20`
- **`create_document`** (Function) — `crates/lingbi-application/src/document_service.rs:32`
- **`read_document`** (Function) — `crates/lingbi-application/src/document_service.rs:66`
- **`list_documents`** (Function) — `crates/lingbi-application/src/document_service.rs:74`
- **`get_document`** (Function) — `crates/lingbi-application/src/document_service.rs:80`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `crates/lingbi-application/src/document_service.rs` | 20 |
| `create_document` | Function | `crates/lingbi-application/src/document_service.rs` | 32 |
| `read_document` | Function | `crates/lingbi-application/src/document_service.rs` | 66 |
| `list_documents` | Function | `crates/lingbi-application/src/document_service.rs` | 74 |
| `get_document` | Function | `crates/lingbi-application/src/document_service.rs` | 80 |
| `save_document` | Function | `crates/lingbi-application/src/document_service.rs` | 84 |
| `recover_pending` | Function | `crates/lingbi-application/src/document_service.rs` | 197 |
| `find_document` | Function | `crates/lingbi-application/src/document_service.rs` | 157 |
| `read_index` | Function | `crates/lingbi-application/src/document_service.rs` | 164 |
| `write_index` | Function | `crates/lingbi-application/src/document_service.rs` | 184 |
| `document_not_found` | Function | `crates/lingbi-application/src/document_service.rs` | 299 |
| `hex_sha256` | Function | `crates/lingbi-application/src/document_service.rs` | 307 |
| `create_read_save_round_trip` | Function | `crates/lingbi-application/src/document_service.rs` | 318 |
| `stale_revision_is_rejected` | Function | `crates/lingbi-application/src/document_service.rs` | 342 |
| `external_content_change_is_conflict_even_when_revision_matches` | Function | `crates/lingbi-application/src/document_service.rs` | 366 |
| `recover_cleans_intent_only_transaction` | Function | `crates/lingbi-application/src/document_service.rs` | 395 |
| `recover_completes_transaction_after_content_write` | Function | `crates/lingbi-application/src/document_service.rs` | 428 |
| `recover_cleans_transaction_after_metadata_write` | Function | `crates/lingbi-application/src/document_service.rs` | 466 |
| `recover_preserves_external_body_after_content_write` | Function | `crates/lingbi-application/src/document_service.rs` | 509 |
| `list_documents_returns_documents_in_order` | Function | `crates/lingbi-application/src/document_service.rs` | 547 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Save_document → Transaction_dir` | cross_community | 5 |
| `Recover_completes_transaction_after_content_write → New` | cross_community | 5 |
| `Recover_completes_transaction_after_content_write → Read` | cross_community | 5 |
| `Recover_completes_transaction_after_content_write → New` | cross_community | 5 |
| `Recover_completes_transaction_after_content_write → Hex_sha256` | cross_community | 5 |
| `Recover_cleans_transaction_after_metadata_write → New` | cross_community | 5 |
| `Recover_cleans_transaction_after_metadata_write → Read` | cross_community | 5 |
| `Recover_cleans_transaction_after_metadata_write → New` | cross_community | 5 |
| `Recover_cleans_transaction_after_metadata_write → Hex_sha256` | cross_community | 5 |
| `Recover_cleans_intent_only_transaction → New` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Cluster_5 | 11 calls |
| Cluster_29 | 4 calls |
| Cluster_26 | 3 calls |

## How to Explore

1. `context({name: "new"})` — see callers and callees
2. `query({search_query: "cluster_4"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
