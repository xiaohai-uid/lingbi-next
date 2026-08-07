---
name: cluster-27
description: "Skill for the Cluster_27 area of lingbi-next. 10 symbols across 1 files."
---

# Cluster_27

10 symbols | 1 files | Cohesion: 100%

## When to Use

- Working with code in `crates/`
- Understanding how new, expose work
- Modifying cluster_27-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/lingbi-security/src/secret_store.rs` | new, expose, put, get, delete (+5) |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `crates/lingbi-security/src/secret_store.rs:10`
- **`expose`** (Function) — `crates/lingbi-security/src/secret_store.rs:14`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `crates/lingbi-security/src/secret_store.rs` | 10 |
| `expose` | Function | `crates/lingbi-security/src/secret_store.rs` | 14 |
| `put` | Function | `crates/lingbi-security/src/secret_store.rs` | 27 |
| `get` | Function | `crates/lingbi-security/src/secret_store.rs` | 28 |
| `delete` | Function | `crates/lingbi-security/src/secret_store.rs` | 29 |
| `default` | Function | `crates/lingbi-security/src/secret_store.rs` | 76 |
| `lock_error` | Function | `crates/lingbi-security/src/secret_store.rs` | 107 |
| `keyring_error` | Function | `crates/lingbi-security/src/secret_store.rs` | 115 |
| `secret_store_put_get_delete_round_trip` | Function | `crates/lingbi-security/src/secret_store.rs` | 128 |
| `secret_string_debug_is_redacted` | Function | `crates/lingbi-security/src/secret_store.rs` | 143 |

## How to Explore

1. `context({name: "new"})` — see callers and callees
2. `query({search_query: "cluster_27"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
