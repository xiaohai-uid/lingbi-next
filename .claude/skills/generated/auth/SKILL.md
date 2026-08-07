---
name: auth
description: "Skill for the Auth area of lingbi-next. 19 symbols across 4 files."
---

# Auth

19 symbols | 4 files | Cohesion: 82%

## When to Use

- Working with code in `services/`
- Understanding how TestEntitlementEndpoint, NewService, TestLoginRefreshLogoutMe work
- Modifying auth-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `services/cloud/internal/auth/service.go` | Register, Login, Refresh, Logout, createTokens (+4) |
| `services/cloud/internal/auth/http.go` | LoginHandler, RefreshHandler, LogoutHandler, MeHandler, writeJSON (+1) |
| `services/cloud/internal/auth/service_test.go` | TestLoginRefreshLogoutMe, TestRefreshTokenIsStoredHashed, TestAuthHandlers |
| `services/cloud/internal/server/server_test.go` | TestEntitlementEndpoint |

## Entry Points

Start here when exploring this area:

- **`TestEntitlementEndpoint`** (Function) — `services/cloud/internal/server/server_test.go:42`
- **`NewService`** (Function) — `services/cloud/internal/auth/service.go:44`
- **`TestLoginRefreshLogoutMe`** (Function) — `services/cloud/internal/auth/service_test.go:10`
- **`TestRefreshTokenIsStoredHashed`** (Function) — `services/cloud/internal/auth/service_test.go:38`
- **`TestAuthHandlers`** (Function) — `services/cloud/internal/auth/service_test.go:54`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `TestEntitlementEndpoint` | Function | `services/cloud/internal/server/server_test.go` | 42 |
| `NewService` | Function | `services/cloud/internal/auth/service.go` | 44 |
| `TestLoginRefreshLogoutMe` | Function | `services/cloud/internal/auth/service_test.go` | 10 |
| `TestRefreshTokenIsStoredHashed` | Function | `services/cloud/internal/auth/service_test.go` | 38 |
| `TestAuthHandlers` | Function | `services/cloud/internal/auth/service_test.go` | 54 |
| `Register` | Method | `services/cloud/internal/auth/service.go` | 55 |
| `Login` | Method | `services/cloud/internal/auth/service.go` | 79 |
| `Refresh` | Method | `services/cloud/internal/auth/service.go` | 97 |
| `Logout` | Method | `services/cloud/internal/auth/service.go` | 110 |
| `LoginHandler` | Method | `services/cloud/internal/auth/http.go` | 27 |
| `RefreshHandler` | Method | `services/cloud/internal/auth/http.go` | 44 |
| `LogoutHandler` | Method | `services/cloud/internal/auth/http.go` | 61 |
| `MeHandler` | Method | `services/cloud/internal/auth/http.go` | 74 |
| `hashToken` | Function | `services/cloud/internal/auth/service.go` | 163 |
| `randomID` | Function | `services/cloud/internal/auth/service.go` | 168 |
| `writeJSON` | Function | `services/cloud/internal/auth/http.go` | 84 |
| `writeError` | Function | `services/cloud/internal/auth/http.go` | 90 |
| `createTokens` | Method | `services/cloud/internal/auth/service.go` | 137 |
| `createTokensLocked` | Method | `services/cloud/internal/auth/service.go` | 143 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `LoginHandler → WriteJSON` | intra_community | 3 |
| `RefreshHandler → WriteJSON` | intra_community | 3 |
| `MeHandler → WriteJSON` | intra_community | 3 |
| `LogoutHandler → WriteJSON` | intra_community | 3 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Server | 2 calls |
| Releases | 1 calls |
| Billing | 1 calls |

## How to Explore

1. `context({name: "TestEntitlementEndpoint"})` — see callers and callees
2. `query({search_query: "auth"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
