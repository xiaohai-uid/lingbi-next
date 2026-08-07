---
name: cluster-29
description: "Skill for the Cluster_29 area of lingbi-next. 10 symbols across 1 files."
---

# Cluster_29

10 symbols | 1 files | Cohesion: 63%

## When to Use

- Working with code in `crates/`
- Understanding how write_atomic, temp_path_for, hex_sha256 work
- Modifying cluster_29-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/lingbi-storage/src/atomic_file.rs` | write_atomic, temp_path_for, hex_sha256, normal_write_is_readable, replacement_replaces_canonical_bytes (+5) |

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `write_atomic` | Function | `crates/lingbi-storage/src/atomic_file.rs` | 10 |
| `temp_path_for` | Function | `crates/lingbi-storage/src/atomic_file.rs` | 124 |
| `hex_sha256` | Function | `crates/lingbi-storage/src/atomic_file.rs` | 136 |
| `normal_write_is_readable` | Function | `crates/lingbi-storage/src/atomic_file.rs` | 147 |
| `replacement_replaces_canonical_bytes` | Function | `crates/lingbi-storage/src/atomic_file.rs` | 161 |
| `hash_conflict_preserves_canonical_bytes` | Function | `crates/lingbi-storage/src/atomic_file.rs` | 173 |
| `expected_hash_compares_against_disk_content_before_write` | Function | `crates/lingbi-storage/src/atomic_file.rs` | 188 |
| `expected_hash_conflict_preserves_external_bytes` | Function | `crates/lingbi-storage/src/atomic_file.rs` | 206 |
| `expected_hash_with_missing_file_does_not_create_content` | Function | `crates/lingbi-storage/src/atomic_file.rs` | 229 |
| `stale_temp_file_does_not_become_canonical` | Function | `crates/lingbi-storage/src/atomic_file.rs` | 241 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Recover_completes_transaction_after_content_write → Hex_sha256` | cross_community | 5 |
| `Recover_cleans_transaction_after_metadata_write → Hex_sha256` | cross_community | 5 |
| `Same_idempotency_key_survives_new_engine → Hex_sha256` | cross_community | 5 |
| `Recover → Hex_sha256` | cross_community | 5 |
| `Recover → Read` | cross_community | 5 |
| `Recover_transaction → Hex_sha256` | cross_community | 5 |
| `Recover_transaction → Read` | cross_community | 5 |
| `Recover_transaction → Temp_path_for` | cross_community | 5 |
| `Complete_transaction → Hex_sha256` | cross_community | 5 |
| `Generate_with_cancel_stream → Hex_sha256` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Cluster_5 | 1 calls |

## How to Explore

1. `context({name: "write_atomic"})` — see callers and callees
2. `query({search_query: "cluster_29"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
