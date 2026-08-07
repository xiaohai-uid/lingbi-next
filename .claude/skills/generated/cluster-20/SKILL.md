---
name: cluster-20
description: "Skill for the Cluster_20 area of lingbi-next. 6 symbols across 1 files."
---

# Cluster_20

6 symbols | 1 files | Cohesion: 86%

## When to Use

- Working with code in `crates/`
- Understanding how import_package work
- Modifying cluster_20-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/lingbi-import-export/src/portable_package.rs` | import_package, validate_relative_path, unsafe_path, unsafe_package_path_is_rejected, checksum_mismatch_is_rejected (+1) |

## Entry Points

Start here when exploring this area:

- **`import_package`** (Function) — `crates/lingbi-import-export/src/portable_package.rs:75`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `import_package` | Function | `crates/lingbi-import-export/src/portable_package.rs` | 75 |
| `validate_relative_path` | Function | `crates/lingbi-import-export/src/portable_package.rs` | 188 |
| `unsafe_path` | Function | `crates/lingbi-import-export/src/portable_package.rs` | 207 |
| `unsafe_package_path_is_rejected` | Function | `crates/lingbi-import-export/src/portable_package.rs` | 282 |
| `checksum_mismatch_is_rejected` | Function | `crates/lingbi-import-export/src/portable_package.rs` | 309 |
| `write_zip` | Function | `crates/lingbi-import-export/src/portable_package.rs` | 335 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Cluster_19 | 1 calls |

## How to Explore

1. `context({name: "import_package"})` — see callers and callees
2. `query({search_query: "cluster_20"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
