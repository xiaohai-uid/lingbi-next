---
name: billing
description: "Skill for the Billing area of lingbi-next. 15 symbols across 6 files."
---

# Billing

15 symbols | 6 files | Cohesion: 89%

## When to Use

- Working with code in `services/`
- Understanding how TestWebhookIsIdempotent, NewCheckoutService, TestCheckoutAndWebhookHandlers work
- Modifying billing-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `services/cloud/internal/billing/http.go` | NewCheckoutService, CheckoutHandler, WebhookHandler, writeJSON, writeError |
| `services/cloud/internal/billing/webhook.go` | NewMemoryEntitlementMutator, NewWebhookService, Apply, Handle |
| `services/cloud/internal/billing/http_test.go` | TestCheckoutAndWebhookHandlers, TestInvalidCheckoutRequestRejected, TestInvalidWebhookBodyRejected |
| `services/cloud/cmd/server/main.go` | main |
| `services/cloud/internal/billing/billing_test.go` | TestWebhookIsIdempotent |
| `services/cloud/internal/server/server_test.go` | newCheckout |

## Entry Points

Start here when exploring this area:

- **`TestWebhookIsIdempotent`** (Function) — `services/cloud/internal/billing/billing_test.go:23`
- **`NewCheckoutService`** (Function) — `services/cloud/internal/billing/http.go:14`
- **`TestCheckoutAndWebhookHandlers`** (Function) — `services/cloud/internal/billing/http_test.go:9`
- **`TestInvalidCheckoutRequestRejected`** (Function) — `services/cloud/internal/billing/http_test.go:36`
- **`TestInvalidWebhookBodyRejected`** (Function) — `services/cloud/internal/billing/http_test.go:46`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `TestWebhookIsIdempotent` | Function | `services/cloud/internal/billing/billing_test.go` | 23 |
| `NewCheckoutService` | Function | `services/cloud/internal/billing/http.go` | 14 |
| `TestCheckoutAndWebhookHandlers` | Function | `services/cloud/internal/billing/http_test.go` | 9 |
| `TestInvalidCheckoutRequestRejected` | Function | `services/cloud/internal/billing/http_test.go` | 36 |
| `TestInvalidWebhookBodyRejected` | Function | `services/cloud/internal/billing/http_test.go` | 46 |
| `NewMemoryEntitlementMutator` | Function | `services/cloud/internal/billing/webhook.go` | 13 |
| `NewWebhookService` | Function | `services/cloud/internal/billing/webhook.go` | 36 |
| `CheckoutHandler` | Method | `services/cloud/internal/billing/http.go` | 28 |
| `WebhookHandler` | Method | `services/cloud/internal/billing/http.go` | 48 |
| `Apply` | Method | `services/cloud/internal/billing/webhook.go` | 17 |
| `Handle` | Method | `services/cloud/internal/billing/webhook.go` | 43 |
| `main` | Function | `services/cloud/cmd/server/main.go` | 14 |
| `newCheckout` | Function | `services/cloud/internal/server/server_test.go` | 68 |
| `writeJSON` | Function | `services/cloud/internal/billing/http.go` | 66 |
| `writeError` | Function | `services/cloud/internal/billing/http.go` | 72 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Main → Service` | cross_community | 3 |
| `Main → MemoryStorage` | cross_community | 3 |
| `Main → CheckoutService` | intra_community | 3 |
| `Main → WebhookService` | intra_community | 3 |
| `CheckoutHandler → WriteJSON` | intra_community | 3 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Releases | 2 calls |
| Server | 1 calls |
| Auth | 1 calls |
| Entitlement | 1 calls |

## How to Explore

1. `context({name: "TestWebhookIsIdempotent"})` — see callers and callees
2. `query({search_query: "billing"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
