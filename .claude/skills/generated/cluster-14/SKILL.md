---
name: cluster-14
description: "Skill for the Cluster_14 area of lingbi-next. 6 symbols across 1 files."
---

# Cluster_14

6 symbols | 1 files | Cohesion: 67%

## When to Use

- Working with code in `crates/`
- Understanding how inspect_v1 work
- Modifying cluster_14-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/lingbi-import-export/src/flutter_v1.rs` | inspect_v1, read_v1_metadata, is_chapter_path, scan_markdown, scan_markdown_recursive (+1) |

## Entry Points

Start here when exploring this area:

- **`inspect_v1`** (Function) — `crates/lingbi-import-export/src/flutter_v1.rs:35`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `inspect_v1` | Function | `crates/lingbi-import-export/src/flutter_v1.rs` | 35 |
| `read_v1_metadata` | Function | `crates/lingbi-import-export/src/flutter_v1.rs` | 152 |
| `is_chapter_path` | Function | `crates/lingbi-import-export/src/flutter_v1.rs` | 190 |
| `scan_markdown` | Function | `crates/lingbi-import-export/src/flutter_v1.rs` | 195 |
| `scan_markdown_recursive` | Function | `crates/lingbi-import-export/src/flutter_v1.rs` | 202 |
| `real_v1_layout_migrates_only_chapter_directories_as_documents` | Function | `crates/lingbi-import-export/src/flutter_v1.rs` | 363 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Cluster_16 | 1 calls |
| Cluster_15 | 1 calls |

## How to Explore

1. `context({name: "inspect_v1"})` — see callers and callees
2. `query({search_query: "cluster_14"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
