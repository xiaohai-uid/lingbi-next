---
name: cluster-5
description: "Skill for the Cluster_5 area of lingbi-next. 17 symbols across 3 files."
---

# Cluster_5

17 symbols | 3 files | Cohesion: 71%

## When to Use

- Working with code in `crates/`
- Understanding how new, begin, set_phase work
- Modifying cluster_5-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/lingbi-storage/src/transaction.rs` | new, begin, set_phase, get, list (+7) |
| `crates/lingbi-application/src/document_service.rs` | read_index_raw, recover_transaction, complete_transaction, mark_failed |
| `crates/lingbi-storage/src/atomic_file.rs` | read |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `crates/lingbi-storage/src/transaction.rs:38`
- **`begin`** (Function) — `crates/lingbi-storage/src/transaction.rs:45`
- **`set_phase`** (Function) — `crates/lingbi-storage/src/transaction.rs:55`
- **`get`** (Function) — `crates/lingbi-storage/src/transaction.rs:74`
- **`list`** (Function) — `crates/lingbi-storage/src/transaction.rs:85`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `crates/lingbi-storage/src/transaction.rs` | 38 |
| `begin` | Function | `crates/lingbi-storage/src/transaction.rs` | 45 |
| `set_phase` | Function | `crates/lingbi-storage/src/transaction.rs` | 55 |
| `get` | Function | `crates/lingbi-storage/src/transaction.rs` | 74 |
| `list` | Function | `crates/lingbi-storage/src/transaction.rs` | 85 |
| `delete` | Function | `crates/lingbi-storage/src/transaction.rs` | 104 |
| `read_index_raw` | Function | `crates/lingbi-application/src/document_service.rs` | 169 |
| `recover_transaction` | Function | `crates/lingbi-application/src/document_service.rs` | 205 |
| `complete_transaction` | Function | `crates/lingbi-application/src/document_service.rs` | 254 |
| `mark_failed` | Function | `crates/lingbi-application/src/document_service.rs` | 292 |
| `read` | Function | `crates/lingbi-storage/src/atomic_file.rs` | 8 |
| `transaction_dir` | Function | `crates/lingbi-storage/src/transaction.rs` | 113 |
| `transaction_path` | Function | `crates/lingbi-storage/src/transaction.rs` | 117 |
| `io_error` | Function | `crates/lingbi-storage/src/transaction.rs` | 122 |
| `parse_error` | Function | `crates/lingbi-storage/src/transaction.rs` | 130 |
| `transaction` | Function | `crates/lingbi-storage/src/transaction.rs` | 143 |
| `transaction_phase_persists_and_deletes` | Function | `crates/lingbi-storage/src/transaction.rs` | 158 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Recover_transaction → Transaction_dir` | intra_community | 6 |
| `Save_document → Transaction_dir` | cross_community | 5 |
| `Recover_completes_transaction_after_content_write → Read` | cross_community | 5 |
| `Recover_completes_transaction_after_content_write → New` | cross_community | 5 |
| `Recover_cleans_transaction_after_metadata_write → Read` | cross_community | 5 |
| `Recover_cleans_transaction_after_metadata_write → New` | cross_community | 5 |
| `Recover_cleans_intent_only_transaction → Read` | cross_community | 5 |
| `Recover_cleans_intent_only_transaction → New` | cross_community | 5 |
| `Recover_preserves_external_body_after_content_write → Read` | cross_community | 5 |
| `Recover_preserves_external_body_after_content_write → New` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Cluster_4 | 5 calls |
| Cluster_29 | 3 calls |

## How to Explore

1. `context({name: "new"})` — see callers and callees
2. `query({search_query: "cluster_5"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
