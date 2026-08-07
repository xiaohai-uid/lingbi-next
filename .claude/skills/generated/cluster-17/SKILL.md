---
name: cluster-17
description: "Skill for the Cluster_17 area of lingbi-next. 9 symbols across 1 files."
---

# Cluster_17

9 symbols | 1 files | Cohesion: 92%

## When to Use

- Working with code in `crates/`
- Understanding how new, import_text, export_docx work
- Modifying cluster_17-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/lingbi-import-export/src/import_export.rs` | new, import_text, export_docx, io_error, zip_error (+4) |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `crates/lingbi-import-export/src/import_export.rs:14`
- **`import_text`** (Function) — `crates/lingbi-import-export/src/import_export.rs:18`
- **`export_docx`** (Function) — `crates/lingbi-import-export/src/import_export.rs:62`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `crates/lingbi-import-export/src/import_export.rs` | 14 |
| `import_text` | Function | `crates/lingbi-import-export/src/import_export.rs` | 18 |
| `export_docx` | Function | `crates/lingbi-import-export/src/import_export.rs` | 62 |
| `io_error` | Function | `crates/lingbi-import-export/src/import_export.rs` | 131 |
| `zip_error` | Function | `crates/lingbi-import-export/src/import_export.rs` | 139 |
| `build_docx_xml` | Function | `crates/lingbi-import-export/src/import_export.rs` | 147 |
| `imports_markdown_and_txt` | Function | `crates/lingbi-import-export/src/import_export.rs` | 181 |
| `rejects_unsupported_import` | Function | `crates/lingbi-import-export/src/import_export.rs` | 204 |
| `exports_docx_with_readable_xml` | Function | `crates/lingbi-import-export/src/import_export.rs` | 238 |

## How to Explore

1. `context({name: "new"})` — see callers and callees
2. `query({search_query: "cluster_17"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
