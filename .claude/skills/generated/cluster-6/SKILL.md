---
name: cluster-6
description: "Skill for the Cluster_6 area of lingbi-next. 15 symbols across 1 files."
---

# Cluster_6

15 symbols | 1 files | Cohesion: 86%

## When to Use

- Working with code in `crates/`
- Understanding how new, generate, generate_with_cancel_stream work
- Modifying cluster_6-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `crates/lingbi-application/src/generation_service.rs` | new, generate, generate_with_cancel_stream, adopt, reject (+10) |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `crates/lingbi-application/src/generation_service.rs:23`
- **`generate`** (Function) — `crates/lingbi-application/src/generation_service.rs:37`
- **`generate_with_cancel_stream`** (Function) — `crates/lingbi-application/src/generation_service.rs:47`
- **`adopt`** (Function) — `crates/lingbi-application/src/generation_service.rs:122`
- **`reject`** (Function) — `crates/lingbi-application/src/generation_service.rs:131`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `crates/lingbi-application/src/generation_service.rs` | 23 |
| `generate` | Function | `crates/lingbi-application/src/generation_service.rs` | 37 |
| `generate_with_cancel_stream` | Function | `crates/lingbi-application/src/generation_service.rs` | 47 |
| `adopt` | Function | `crates/lingbi-application/src/generation_service.rs` | 122 |
| `reject` | Function | `crates/lingbi-application/src/generation_service.rs` | 131 |
| `read_candidate` | Function | `crates/lingbi-application/src/generation_service.rs` | 137 |
| `write_candidate` | Function | `crates/lingbi-application/src/generation_service.rs` | 147 |
| `ai_error` | Function | `crates/lingbi-application/src/generation_service.rs` | 152 |
| `hex_sha256` | Function | `crates/lingbi-application/src/generation_service.rs` | 166 |
| `setup` | Function | `crates/lingbi-application/src/generation_service.rs` | 178 |
| `fake_provider_creates_candidate_without_canonical_write` | Function | `crates/lingbi-application/src/generation_service.rs` | 197 |
| `provider_error_creates_no_candidate` | Function | `crates/lingbi-application/src/generation_service.rs` | 225 |
| `adopt_updates_canonical_content_and_survives_restart` | Function | `crates/lingbi-application/src/generation_service.rs` | 244 |
| `stale_candidate_is_rejected_and_user_edits_survive` | Function | `crates/lingbi-application/src/generation_service.rs` | 286 |
| `consecutive_candidate_adoptions_advance_revisions` | Function | `crates/lingbi-application/src/generation_service.rs` | 330 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Generate_with_cancel_stream → Candidate_dir` | cross_community | 5 |
| `Generate_with_cancel_stream → Hex_sha256` | cross_community | 5 |
| `Generate_with_cancel_stream → Read` | cross_community | 5 |
| `Generate_with_cancel_stream → Temp_path_for` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Cluster_7 | 3 calls |
| Tests | 2 calls |

## How to Explore

1. `context({name: "new"})` — see callers and callees
2. `query({search_query: "cluster_6"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
