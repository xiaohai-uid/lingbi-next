---
name: tests
description: "Skill for the Tests area of lingbi-next. 61 symbols across 10 files."
---

# Tests

61 symbols | 10 files | Cohesion: 85%

## When to Use

- Working with code in `crates/`
- Understanding how new, create_project, open_project work
- Modifying tests-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `e2e/desktop/tests/real_desktop_binary_e2e.rs` | new, execute, execute_async, post, delete (+14) |
| `crates/lingbi-application/src/project_service.rs` | new, create_project, open_project, default, create_first_document (+8) |
| `crates/lingbi-ai/src/ai.rs` | new, cancel, test_connection, stream_chat, unconfigured (+6) |
| `crates/lingbi-ffi/src/lib.rs` | lingbi_open_project_json, lingbi_free_string, to_c_string, error_json, open_project_json_returns_v2_session |
| `crates/lingbi-recovery/src/recovery.rs` | scan, scan_candidates, scan_intents, scan_content_hashes, hex_sha256 |
| `e2e/desktop/tests/recovery_crash_failpoints.rs` | hex_sha256, setup_failpoint, verify_recovered, every_crash_failpoint_recovers_and_project_still_opens |
| `crates/lingbi-application/tests/project_v2_fixture.rs` | rust_opens_shared_project_v2_fixture |
| `e2e/desktop/tests/real_golden_path.rs` | real_desktop_golden_path |
| `e2e/desktop/tests/offline_behavior.rs` | offline_manuscript_stays_usable_when_ai_fails |
| `e2e/desktop/tests/candidate_recovery.rs` | generated_candidate_scans_without_json_parse_error |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `crates/lingbi-application/src/project_service.rs:29`
- **`create_project`** (Function) — `crates/lingbi-application/src/project_service.rs:35`
- **`open_project`** (Function) — `crates/lingbi-application/src/project_service.rs:79`
- **`new`** (Function) — `crates/lingbi-ai/src/ai.rs:88`
- **`cancel`** (Function) — `crates/lingbi-ai/src/ai.rs:92`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `crates/lingbi-application/src/project_service.rs` | 29 |
| `create_project` | Function | `crates/lingbi-application/src/project_service.rs` | 35 |
| `open_project` | Function | `crates/lingbi-application/src/project_service.rs` | 79 |
| `new` | Function | `crates/lingbi-ai/src/ai.rs` | 88 |
| `cancel` | Function | `crates/lingbi-ai/src/ai.rs` | 92 |
| `unconfigured` | Function | `crates/lingbi-ai/src/ai.rs` | 186 |
| `with_error` | Function | `crates/lingbi-ai/src/ai.rs` | 523 |
| `lingbi_open_project_json` | Function | `crates/lingbi-ffi/src/lib.rs` | 18 |
| `lingbi_free_string` | Function | `crates/lingbi-ffi/src/lib.rs` | 55 |
| `scan` | Function | `crates/lingbi-recovery/src/recovery.rs` | 59 |
| `new` | Function | `e2e/desktop/tests/real_desktop_binary_e2e.rs` | 25 |
| `execute` | Function | `e2e/desktop/tests/real_desktop_binary_e2e.rs` | 33 |
| `execute_async` | Function | `e2e/desktop/tests/real_desktop_binary_e2e.rs` | 40 |
| `post` | Function | `e2e/desktop/tests/real_desktop_binary_e2e.rs` | 47 |
| `delete` | Function | `e2e/desktop/tests/real_desktop_binary_e2e.rs` | 58 |
| `create_session` | Function | `e2e/desktop/tests/real_desktop_binary_e2e.rs` | 66 |
| `wait_until` | Function | `e2e/desktop/tests/real_desktop_binary_e2e.rs` | 99 |
| `click_button` | Function | `e2e/desktop/tests/real_desktop_binary_e2e.rs` | 114 |
| `set_welcome_inputs` | Function | `e2e/desktop/tests/real_desktop_binary_e2e.rs` | 128 |
| `wait_for_text` | Function | `e2e/desktop/tests/real_desktop_binary_e2e.rs` | 146 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Open_project_recovers_content_written_transaction → Transaction_dir` | cross_community | 5 |
| `Open_project_recovers_content_written_transaction → New` | cross_community | 5 |
| `Open_project_recovers_content_written_transaction → Read` | cross_community | 5 |
| `Open_project_recovers_content_written_transaction → New` | intra_community | 4 |
| `Open_project_recovers_content_written_transaction → Document` | intra_community | 4 |
| `Open_project_recovers_content_written_transaction → Hex_sha256` | intra_community | 4 |
| `Open_project_recovers_content_written_transaction → Extract_title` | intra_community | 4 |
| `Detects_external_bytes_changed → New` | cross_community | 4 |
| `Detects_external_bytes_changed → RecoveryIncident` | cross_community | 4 |
| `Detects_external_bytes_changed → Read` | cross_community | 4 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Cluster_4 | 4 calls |
| Cluster_6 | 4 calls |
| Cluster_10 | 3 calls |
| Cluster_5 | 3 calls |

## How to Explore

1. `context({name: "new"})` — see callers and callees
2. `query({search_query: "tests"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
