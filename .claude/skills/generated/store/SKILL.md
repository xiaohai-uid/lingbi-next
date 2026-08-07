---
name: store
description: "Skill for the Store area of lingbi-next. 34 symbols across 3 files."
---

# Store

34 symbols | 3 files | Cohesion: 100%

## When to Use

- Working with code in `apps/`
- Understanding how toCommandError, useAppStore, App work
- Modifying store-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `apps/desktop/src/lib/desktop.ts` | toCommandError, inTauri, generateStreaming, cleanup, createProject (+12) |
| `apps/desktop/src/store/useAppStore.ts` | createProject, openProject, saveDocument, createChapter, selectDocument (+5) |
| `apps/desktop/src/App.tsx` | Welcome, createProject, openProject, Editor, generate (+2) |

## Entry Points

Start here when exploring this area:

- **`toCommandError`** (Function) — `apps/desktop/src/lib/desktop.ts:60`
- **`useAppStore`** (Function) — `apps/desktop/src/store/useAppStore.ts:30`
- **`App`** (Function) — `apps/desktop/src/App.tsx:119`
- **`createProject`** (Method) — `apps/desktop/src/lib/desktop.ts:134`
- **`openProject`** (Method) — `apps/desktop/src/lib/desktop.ts:166`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `toCommandError` | Function | `apps/desktop/src/lib/desktop.ts` | 60 |
| `useAppStore` | Function | `apps/desktop/src/store/useAppStore.ts` | 30 |
| `App` | Function | `apps/desktop/src/App.tsx` | 119 |
| `createProject` | Method | `apps/desktop/src/lib/desktop.ts` | 134 |
| `openProject` | Method | `apps/desktop/src/lib/desktop.ts` | 166 |
| `getSession` | Method | `apps/desktop/src/lib/desktop.ts` | 175 |
| `listDocuments` | Method | `apps/desktop/src/lib/desktop.ts` | 182 |
| `createDocument` | Method | `apps/desktop/src/lib/desktop.ts` | 191 |
| `openDocument` | Method | `apps/desktop/src/lib/desktop.ts` | 223 |
| `saveDocument` | Method | `apps/desktop/src/lib/desktop.ts` | 230 |
| `providerConfigure` | Method | `apps/desktop/src/lib/desktop.ts` | 251 |
| `generate` | Method | `apps/desktop/src/lib/desktop.ts` | 265 |
| `generationCancel` | Method | `apps/desktop/src/lib/desktop.ts` | 292 |
| `candidateList` | Method | `apps/desktop/src/lib/desktop.ts` | 298 |
| `candidateAdopt` | Method | `apps/desktop/src/lib/desktop.ts` | 305 |
| `candidateReject` | Method | `apps/desktop/src/lib/desktop.ts` | 324 |
| `createProject` | Method | `apps/desktop/src/store/useAppStore.ts` | 41 |
| `openProject` | Method | `apps/desktop/src/store/useAppStore.ts` | 59 |
| `saveDocument` | Method | `apps/desktop/src/store/useAppStore.ts` | 71 |
| `createChapter` | Method | `apps/desktop/src/store/useAppStore.ts` | 97 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `App → UseAppStore` | intra_community | 4 |
| `App → CreateProject` | intra_community | 4 |
| `App → OpenProject` | intra_community | 4 |
| `CreateProject → InTauri` | intra_community | 3 |
| `OpenProject → InTauri` | intra_community | 3 |
| `CreateChapter → InTauri` | intra_community | 3 |

## How to Explore

1. `context({name: "toCommandError"})` — see callers and callees
2. `query({search_query: "store"})` — find related execution flows
3. Read key files listed above for implementation details
4. `explain({target: "<file or symbol>"})` — persisted taint findings (source→sink data flows), when indexed with `--pdg`
