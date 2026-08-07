---
name: cluster-30
description: "Skill for the Cluster_30 area of lingbi-next. 8 symbols across 1 files."
---

# Cluster_30

8 symbols | 1 files | Cohesion: 85%

## When to Use

- Working with code in `crates/`
- Understanding how new, list, find work
- Modifying cluster_30-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/lingbi-storage/src/document_repository.rs` | new, list, find, update, write (+3) |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `crates/lingbi-storage/src/document_repository.rs:13`
- **`list`** (Function) — `crates/lingbi-storage/src/document_repository.rs:20`
- **`find`** (Function) — `crates/lingbi-storage/src/document_repository.rs:29`
- **`update`** (Function) — `crates/lingbi-storage/src/document_repository.rs:33`
- **`write`** (Function) — `crates/lingbi-storage/src/document_repository.rs:46`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `crates/lingbi-storage/src/document_repository.rs` | 13 |
| `list` | Function | `crates/lingbi-storage/src/document_repository.rs` | 20 |
| `find` | Function | `crates/lingbi-storage/src/document_repository.rs` | 29 |
| `update` | Function | `crates/lingbi-storage/src/document_repository.rs` | 33 |
| `write` | Function | `crates/lingbi-storage/src/document_repository.rs` | 46 |
| `index_path` | Function | `crates/lingbi-storage/src/document_repository.rs` | 53 |
| `parse_error` | Function | `crates/lingbi-storage/src/document_repository.rs` | 58 |
| `document_repository_reads_index` | Function | `crates/lingbi-storage/src/document_repository.rs` | 74 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Commit → Index_path` | cross_community | 4 |
| `Update → Hex_sha256` | cross_community | 4 |
| `Update → Read` | cross_community | 4 |
| `Update → Temp_path_for` | cross_community | 4 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Cluster_5 | 1 calls |
| Cluster_29 | 1 calls |

## How to Explore

1. `context({name: "new"})` — see callers and callees
2. `query({search_query: "cluster_30"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
