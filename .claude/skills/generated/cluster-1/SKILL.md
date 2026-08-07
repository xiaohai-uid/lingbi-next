---
name: cluster-1
description: "Skill for the Cluster_1 area of lingbi-next. 21 symbols across 2 files."
---

# Cluster_1

21 symbols | 2 files | Cohesion: 98%

## When to Use

- Working with code in `apps/`
- Understanding how is_cancelled work
- Modifying cluster_1-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `apps/desktop/src-tauri/src/lib.rs` | command_error, parse_uuid, project_get_session, document_list, document_create (+15) |
| `crates/lingbi-ai/src/ai.rs` | is_cancelled |

## Entry Points

Start here when exploring this area:

- **`is_cancelled`** (Function) — `crates/lingbi-ai/src/ai.rs:97`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `is_cancelled` | Function | `crates/lingbi-ai/src/ai.rs` | 97 |
| `command_error` | Function | `apps/desktop/src-tauri/src/lib.rs` | 68 |
| `parse_uuid` | Function | `apps/desktop/src-tauri/src/lib.rs` | 76 |
| `project_get_session` | Function | `apps/desktop/src-tauri/src/lib.rs` | 166 |
| `document_list` | Function | `apps/desktop/src-tauri/src/lib.rs` | 178 |
| `document_create` | Function | `apps/desktop/src-tauri/src/lib.rs` | 185 |
| `document_open` | Function | `apps/desktop/src-tauri/src/lib.rs` | 203 |
| `document_save` | Function | `apps/desktop/src-tauri/src/lib.rs` | 222 |
| `provider_test` | Function | `apps/desktop/src-tauri/src/lib.rs` | 281 |
| `generation_start` | Function | `apps/desktop/src-tauri/src/lib.rs` | 294 |
| `candidate_list` | Function | `apps/desktop/src-tauri/src/lib.rs` | 375 |
| `candidate_adopt` | Function | `apps/desktop/src-tauri/src/lib.rs` | 386 |
| `candidate_reject` | Function | `apps/desktop/src-tauri/src/lib.rs` | 403 |
| `generation_cancel` | Function | `apps/desktop/src-tauri/src/lib.rs` | 414 |
| `generation_status` | Function | `apps/desktop/src-tauri/src/lib.rs` | 434 |
| `current_root` | Function | `apps/desktop/src-tauri/src/lib.rs` | 442 |
| `set_current_document` | Function | `apps/desktop/src-tauri/src/lib.rs` | 452 |
| `document_service` | Function | `apps/desktop/src-tauri/src/lib.rs` | 466 |
| `generation_service` | Function | `apps/desktop/src-tauri/src/lib.rs` | 485 |
| `configured_provider` | Function | `apps/desktop/src-tauri/src/lib.rs` | 504 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Generation_start → CommandErrorDto` | intra_community | 4 |

## How to Explore

1. `context({name: "is_cancelled"})` — see callers and callees
2. `query({search_query: "cluster_1"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
