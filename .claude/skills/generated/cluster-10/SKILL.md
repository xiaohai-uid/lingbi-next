---
name: cluster-10
description: "Skill for the Cluster_10 area of lingbi-next. 18 symbols across 3 files."
---

# Cluster_10

18 symbols | 3 files | Cohesion: 79%

## When to Use

- Working with code in `crates/`
- Understanding how commit, physical_path, new work
- Modifying cluster_10-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/lingbi-recovery/src/recovery.rs` | new, recover_all, write_metadata_and_receipt, write_receipt_and_commit, read_intents (+11) |
| `crates/lingbi-domain/src/candidate.rs` | commit |
| `crates/lingbi-domain/src/document.rs` | physical_path |

## Entry Points

Start here when exploring this area:

- **`commit`** (Function) — `crates/lingbi-domain/src/candidate.rs:39`
- **`physical_path`** (Function) — `crates/lingbi-domain/src/document.rs:18`
- **`new`** (Function) — `crates/lingbi-recovery/src/recovery.rs:52`
- **`recover_all`** (Function) — `crates/lingbi-recovery/src/recovery.rs:109`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `commit` | Function | `crates/lingbi-domain/src/candidate.rs` | 39 |
| `physical_path` | Function | `crates/lingbi-domain/src/document.rs` | 18 |
| `new` | Function | `crates/lingbi-recovery/src/recovery.rs` | 52 |
| `recover_all` | Function | `crates/lingbi-recovery/src/recovery.rs` | 109 |
| `write_metadata_and_receipt` | Function | `crates/lingbi-recovery/src/recovery.rs` | 218 |
| `write_receipt_and_commit` | Function | `crates/lingbi-recovery/src/recovery.rs` | 232 |
| `read_intents` | Function | `crates/lingbi-recovery/src/recovery.rs` | 253 |
| `parse_error` | Function | `crates/lingbi-recovery/src/recovery.rs` | 373 |
| `io_error` | Function | `crates/lingbi-recovery/src/recovery.rs` | 381 |
| `recovery_fixture` | Function | `crates/lingbi-recovery/src/recovery.rs` | 429 |
| `assert_recovered` | Function | `crates/lingbi-recovery/src/recovery.rs` | 498 |
| `detects_external_bytes_changed` | Function | `crates/lingbi-recovery/src/recovery.rs` | 589 |
| `recovers_after_intent` | Function | `crates/lingbi-recovery/src/recovery.rs` | 621 |
| `recovers_after_content_write` | Function | `crates/lingbi-recovery/src/recovery.rs` | 633 |
| `recovers_after_metadata_write` | Function | `crates/lingbi-recovery/src/recovery.rs` | 645 |
| `recovers_before_receipt` | Function | `crates/lingbi-recovery/src/recovery.rs` | 657 |
| `external_body_is_preserved_by_recovery` | Function | `crates/lingbi-recovery/src/recovery.rs` | 669 |
| `recovery_prefers_preserving_user_bytes` | Function | `crates/lingbi-recovery/src/recovery.rs` | 697 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Detects_external_bytes_changed → New` | cross_community | 4 |
| `Detects_external_bytes_changed → RecoveryIncident` | cross_community | 4 |
| `Detects_external_bytes_changed → Read` | cross_community | 4 |
| `Detects_external_bytes_changed → Hex_sha256` | cross_community | 4 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Cluster_7 | 2 calls |
| Tests | 2 calls |
| Cluster_24 | 2 calls |
| Cluster_5 | 1 calls |

## How to Explore

1. `context({name: "commit"})` — see callers and callees
2. `query({search_query: "cluster_10"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
