---
name: cluster-21
description: "Skill for the Cluster_21 area of lingbi-next. 11 symbols across 1 files."
---

# Cluster_21

11 symbols | 1 files | Cohesion: 83%

## When to Use

- Working with code in `crates/`
- Understanding how new work
- Modifying cluster_21-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/lingbi-mutation/src/mutation.rs` | new, parse_error, hex_sha256, candidate, setup_engine (+6) |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `crates/lingbi-mutation/src/mutation.rs:47`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `crates/lingbi-mutation/src/mutation.rs` | 47 |
| `parse_error` | Function | `crates/lingbi-mutation/src/mutation.rs` | 349 |
| `hex_sha256` | Function | `crates/lingbi-mutation/src/mutation.rs` | 357 |
| `candidate` | Function | `crates/lingbi-mutation/src/mutation.rs` | 369 |
| `setup_engine` | Function | `crates/lingbi-mutation/src/mutation.rs` | 388 |
| `unapproved_commit_is_rejected` | Function | `crates/lingbi-mutation/src/mutation.rs` | 413 |
| `revision_conflict_is_rejected` | Function | `crates/lingbi-mutation/src/mutation.rs` | 448 |
| `approved_candidate_is_committed_and_persisted` | Function | `crates/lingbi-mutation/src/mutation.rs` | 475 |
| `same_idempotency_key_survives_new_engine` | Function | `crates/lingbi-mutation/src/mutation.rs` | 528 |
| `propose_and_approve_survive_new_engine` | Function | `crates/lingbi-mutation/src/mutation.rs` | 567 |
| `stale_temp_never_becomes_canonical` | Function | `crates/lingbi-mutation/src/mutation.rs` | 593 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Same_idempotency_key_survives_new_engine → Hex_sha256` | cross_community | 5 |
| `Recover_intent → New` | cross_community | 4 |
| `Same_idempotency_key_survives_new_engine → Approval_path` | cross_community | 4 |
| `Same_idempotency_key_survives_new_engine → Intent_path` | cross_community | 4 |
| `Same_idempotency_key_survives_new_engine → Receipt_path` | cross_community | 4 |
| `Stale_temp_never_becomes_canonical → Approval_path` | cross_community | 4 |
| `Stale_temp_never_becomes_canonical → Intent_path` | cross_community | 4 |
| `Stale_temp_never_becomes_canonical → Hex_sha256` | cross_community | 4 |
| `Stale_temp_never_becomes_canonical → Read` | cross_community | 4 |
| `Stale_temp_never_becomes_canonical → Temp_path_for` | cross_community | 4 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Cluster_7 | 4 calls |
| Cluster_10 | 1 calls |

## How to Explore

1. `context({name: "new"})` — see callers and callees
2. `query({search_query: "cluster_21"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
