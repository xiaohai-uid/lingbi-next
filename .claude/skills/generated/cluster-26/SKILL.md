---
name: cluster-26
description: "Skill for the Cluster_26 area of lingbi-next. 13 symbols across 1 files."
---

# Cluster_26

13 symbols | 1 files | Cohesion: 88%

## When to Use

- Working with code in `crates/`
- Understanding how new, resolve work
- Modifying cluster_26-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/lingbi-security/src/project_path.rs` | new, resolve, validate_relative, canonicalize_existing_or_parent, unsafe_path (+8) |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `crates/lingbi-security/src/project_path.rs:9`
- **`resolve`** (Function) — `crates/lingbi-security/src/project_path.rs:17`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `crates/lingbi-security/src/project_path.rs` | 9 |
| `resolve` | Function | `crates/lingbi-security/src/project_path.rs` | 17 |
| `validate_relative` | Function | `crates/lingbi-security/src/project_path.rs` | 48 |
| `canonicalize_existing_or_parent` | Function | `crates/lingbi-security/src/project_path.rs` | 73 |
| `unsafe_path` | Function | `crates/lingbi-security/src/project_path.rs` | 104 |
| `guard` | Function | `crates/lingbi-security/src/project_path.rs` | 113 |
| `accepts_normal_relative_path` | Function | `crates/lingbi-security/src/project_path.rs` | 121 |
| `rejects_windows_parent_traversal` | Function | `crates/lingbi-security/src/project_path.rs` | 130 |
| `rejects_windows_drive_prefix` | Function | `crates/lingbi-security/src/project_path.rs` | 136 |
| `rejects_unc_path` | Function | `crates/lingbi-security/src/project_path.rs` | 142 |
| `rejects_traversal_after_chapter_prefix` | Function | `crates/lingbi-security/src/project_path.rs` | 148 |
| `rejects_nul_byte` | Function | `crates/lingbi-security/src/project_path.rs` | 158 |
| `rejects_symlink_escape` | Function | `crates/lingbi-security/src/project_path.rs` | 165 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Recover_completes_transaction_after_content_write → New` | cross_community | 5 |
| `Recover_cleans_transaction_after_metadata_write → New` | cross_community | 5 |
| `Recover_cleans_intent_only_transaction → New` | cross_community | 5 |
| `Recover_preserves_external_body_after_content_write → New` | cross_community | 5 |
| `Save_document → New` | cross_community | 4 |

## How to Explore

1. `context({name: "new"})` — see callers and callees
2. `query({search_query: "cluster_26"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
