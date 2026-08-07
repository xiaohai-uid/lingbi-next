---
name: cluster-7
description: "Skill for the Cluster_7 area of lingbi-next. 26 symbols across 4 files."
---

# Cluster_7

26 symbols | 4 files | Cohesion: 75%

## When to Use

- Working with code in `crates/`
- Understanding how new, approve_and_commit, write work
- Modifying cluster_7-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/lingbi-mutation/src/mutation.rs` | write, read, approval_path, intent_path, delete (+6) |
| `crates/lingbi-storage/src/candidate.rs` | new, write, read, list, delete (+6) |
| `crates/lingbi-application/src/mutation_coordinator.rs` | new, approve_and_commit |
| `crates/lingbi-recovery/src/recovery.rs` | recover, recover_intent |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `crates/lingbi-application/src/mutation_coordinator.rs:18`
- **`approve_and_commit`** (Function) — `crates/lingbi-application/src/mutation_coordinator.rs:27`
- **`write`** (Function) — `crates/lingbi-mutation/src/mutation.rs:54`
- **`read`** (Function) — `crates/lingbi-mutation/src/mutation.rs:64`
- **`delete`** (Function) — `crates/lingbi-mutation/src/mutation.rs:109`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `crates/lingbi-application/src/mutation_coordinator.rs` | 18 |
| `approve_and_commit` | Function | `crates/lingbi-application/src/mutation_coordinator.rs` | 27 |
| `write` | Function | `crates/lingbi-mutation/src/mutation.rs` | 54 |
| `read` | Function | `crates/lingbi-mutation/src/mutation.rs` | 64 |
| `delete` | Function | `crates/lingbi-mutation/src/mutation.rs` | 109 |
| `find_by_idempotency_key` | Function | `crates/lingbi-mutation/src/mutation.rs` | 153 |
| `propose` | Function | `crates/lingbi-mutation/src/mutation.rs` | 210 |
| `approve` | Function | `crates/lingbi-mutation/src/mutation.rs` | 216 |
| `commit` | Function | `crates/lingbi-mutation/src/mutation.rs` | 247 |
| `recover` | Function | `crates/lingbi-recovery/src/recovery.rs` | 67 |
| `new` | Function | `crates/lingbi-storage/src/candidate.rs` | 14 |
| `write` | Function | `crates/lingbi-storage/src/candidate.rs` | 21 |
| `read` | Function | `crates/lingbi-storage/src/candidate.rs` | 31 |
| `list` | Function | `crates/lingbi-storage/src/candidate.rs` | 42 |
| `delete` | Function | `crates/lingbi-storage/src/candidate.rs` | 60 |
| `approval_path` | Function | `crates/lingbi-mutation/src/mutation.rs` | 75 |
| `intent_path` | Function | `crates/lingbi-mutation/src/mutation.rs` | 105 |
| `receipt_path` | Function | `crates/lingbi-mutation/src/mutation.rs` | 176 |
| `io_error` | Function | `crates/lingbi-mutation/src/mutation.rs` | 341 |
| `recover_intent` | Function | `crates/lingbi-recovery/src/recovery.rs` | 117 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Same_idempotency_key_survives_new_engine → Hex_sha256` | cross_community | 5 |
| `Recover → Candidate_dir` | intra_community | 5 |
| `Recover → Hex_sha256` | cross_community | 5 |
| `Recover → Read` | cross_community | 5 |
| `Generate_with_cancel_stream → Candidate_dir` | cross_community | 5 |
| `Generate_with_cancel_stream → Hex_sha256` | cross_community | 5 |
| `Generate_with_cancel_stream → Read` | cross_community | 5 |
| `Generate_with_cancel_stream → Temp_path_for` | cross_community | 5 |
| `Commit → Read` | cross_community | 4 |
| `Commit → Temp_path_for` | cross_community | 4 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Cluster_29 | 6 calls |
| Cluster_10 | 5 calls |
| Cluster_21 | 4 calls |
| Cluster_5 | 3 calls |
| Cluster_30 | 2 calls |
| Cluster_26 | 1 calls |
| Tests | 1 calls |

## How to Explore

1. `context({name: "new"})` — see callers and callees
2. `query({search_query: "cluster_7"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
