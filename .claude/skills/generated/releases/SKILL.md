---
name: releases
description: "Skill for the Releases area of lingbi-next. 11 symbols across 4 files."
---

# Releases

11 symbols | 4 files | Cohesion: 86%

## When to Use

- Working with code in `services/`
- Understanding how NewService, TestReleaseHandlers, NewMemoryStorage work
- Modifying releases-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `services/cloud/internal/releases/http.go` | LatestHandler, ByVersionHandler, WindowsDownloadHandler, writeJSON, writeError (+1) |
| `services/cloud/internal/releases/storage.go` | NewMemoryStorage, Latest, WindowsX86_64 |
| `services/cloud/internal/releases/http_test.go` | TestReleaseHandlers |
| `services/cloud/internal/server/server_test.go` | newReleases |

## Entry Points

Start here when exploring this area:

- **`NewService`** (Function) — `services/cloud/internal/releases/http.go:13`
- **`TestReleaseHandlers`** (Function) — `services/cloud/internal/releases/http_test.go:10`
- **`NewMemoryStorage`** (Function) — `services/cloud/internal/releases/storage.go:24`
- **`LatestHandler`** (Method) — `services/cloud/internal/releases/http.go:17`
- **`ByVersionHandler`** (Method) — `services/cloud/internal/releases/http.go:26`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `NewService` | Function | `services/cloud/internal/releases/http.go` | 13 |
| `TestReleaseHandlers` | Function | `services/cloud/internal/releases/http_test.go` | 10 |
| `NewMemoryStorage` | Function | `services/cloud/internal/releases/storage.go` | 24 |
| `LatestHandler` | Method | `services/cloud/internal/releases/http.go` | 17 |
| `ByVersionHandler` | Method | `services/cloud/internal/releases/http.go` | 26 |
| `WindowsDownloadHandler` | Method | `services/cloud/internal/releases/http.go` | 36 |
| `Latest` | Method | `services/cloud/internal/releases/storage.go` | 28 |
| `WindowsX86_64` | Method | `services/cloud/internal/releases/storage.go` | 48 |
| `writeJSON` | Function | `services/cloud/internal/releases/http.go` | 45 |
| `writeError` | Function | `services/cloud/internal/releases/http.go` | 51 |
| `newReleases` | Function | `services/cloud/internal/server/server_test.go` | 62 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Main → Service` | cross_community | 3 |
| `Main → MemoryStorage` | cross_community | 3 |
| `LatestHandler → WriteJSON` | intra_community | 3 |
| `ByVersionHandler → WriteJSON` | intra_community | 3 |
| `WindowsDownloadHandler → WriteJSON` | intra_community | 3 |

## How to Explore

1. `context({name: "NewService"})` — see callers and callees
2. `query({search_query: "releases"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
