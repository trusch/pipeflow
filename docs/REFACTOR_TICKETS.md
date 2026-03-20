# Pipeflow refactor tickets

This breaks the architectural cleanup into small, reviewable tickets instead of one giant rewrite.

## Goals

- shrink the biggest modules without changing behavior
- separate rendering/state/orchestration concerns more clearly
- make remote and headless behavior share more code
- tighten security around remote control before adding more remote features

---

## Phase 1 — low-risk file splits

### PF-REF-01 — Split `app/mod.rs` into focused submodules
**Status:** done

**Scope**
- move app-local data types into `src/app/types.rs`
- move status/persistent issue rendering into `src/app/feedback.rs`
- move snapshot capture/restore workflows into `src/app/snapshots.rs`
- keep `PipeflowApp` as the public coordination shell

**Acceptance criteria**
- behavior unchanged
- `cargo fmt`, `clippy`, `test` pass
- `app/mod.rs` is materially smaller and easier to scan

### PF-REF-02 — Split `ui/graph_view.rs` helper/types/geometry code
**Status:** done

**Scope**
- extract graph-view helper functions into `src/ui/graph_view/helpers.rs`
- extract shared types into `src/ui/graph_view/types.rs`
- extract transform and bezier helpers into `src/ui/graph_view/geometry.rs`
- keep the main view logic in `src/ui/graph_view.rs` for now

**Acceptance criteria**
- no UI behavior changes
- `GraphViewResponse` remains public from `crate::ui::graph_view`
- tests still pass

### PF-REF-03 — Document the next refactor layers before touching behavior-heavy code
**Status:** done

**Scope**
- write this ticket plan into the repo
- define sequence for next changes so follow-up work is reviewable

**Acceptance criteria**
- roadmap checked into the repo
- tickets are small enough to land separately

---

## Phase 2 — state and graph-view structure

### PF-REF-04 — Split `core/state.rs`
**Status:** planned

**Scope**
- move `GraphState` into `src/core/state/graph.rs`
- move `UiState` into `src/core/state/ui.rs`
- move persistence serialization helpers into `src/core/state/persistence.rs`
- keep `mod.rs` as assembly + shared exports

**Acceptance criteria**
- public API shape mostly preserved
- no behavioral changes
- state tests stay green

### PF-REF-05 — Split `ui/graph_view.rs` by responsibility
**Status:** planned

**Scope**
- extract interaction code to `interaction.rs`
- extract drawing code to `rendering.rs`
- extract minimap to `minimap.rs`
- extract link drawing and hit testing to `links.rs`

**Acceptance criteria**
- no behavior changes
- file size for `graph_view.rs` drops significantly
- each submodule has a clear responsibility boundary

### PF-REF-06 — Extract snapshot restore diff logic into a domain/service layer
**Status:** planned

**Scope**
- move matching/diff planning for snapshot restore out of the app UI shell
- return a restore plan (`remove links`, `create links`, `set volumes`, `unresolved`)
- keep app layer responsible only for execution and feedback messaging

**Acceptance criteria**
- restore behavior remains identical
- logic becomes unit-testable without UI shell wiring

---

## Phase 3 — shared event processing and remote hardening

### PF-REF-07 — Introduce shared event reducer for GUI + headless
**Status:** planned

**Scope**
- create a shared reducer for `PwEvent` and meter batch application
- have both GUI and headless use the same state mutation logic
- reduce duplication between `app/event_processing.rs` and `headless.rs`

**Acceptance criteria**
- no drift between local and headless state mutation paths
- shared tests cover node/port/link/volume/connect/disconnect flows

### PF-REF-08 — Harden gRPC auth/session enforcement
**Status:** planned

**Scope**
- require auth on all state/control/stream RPCs
- make session or metadata-based auth explicit
- ensure remote auto-start passes token consistently
- add unauthenticated-access tests

**Acceptance criteria**
- unauthenticated `get_state`, `execute_command`, and subscriptions fail
- token handling is consistent across client/server/SSH bootstrap paths

### PF-REF-09 — Make remote startup deterministic
**Status:** planned

**Scope**
- replace fixed sleeps with readiness probing / timeout-based startup
- distinguish tunnel-up, server-up, and authenticated states
- improve error reporting for startup failures

**Acceptance criteria**
- remote startup succeeds or fails with a concrete reason
- no blind `sleep(1)` dependency in the happy path

---

## Phase 4 — cleanup and deletion of drift

### PF-REF-10 — Remove dead/internal API drift
**Status:** planned

**Scope**
- remove helpers that are no longer used
- or wire them back into the UI if they are part of an intended feature
- reduce `allow(dead_code)` usage added during release hardening

**Acceptance criteria**
- clippy clean with fewer suppressions
- no orphaned feature surface left behind

### PF-REF-11 — Add integration tests for product-critical flows
**Status:** planned

**Scope**
- snapshot capture/restore flow
- rule replay flow
- safety enforcement over remote control
- remote reconnect/auth failures

**Acceptance criteria**
- tests cover real product promises, not just conversion helpers

---

## Recommended landing order

1. PF-REF-01
2. PF-REF-02
3. PF-REF-04
4. PF-REF-05
5. PF-REF-06
6. PF-REF-07
7. PF-REF-08
8. PF-REF-09
9. PF-REF-10
10. PF-REF-11

## Notes

- Prefer refactor-only PRs with zero intentional behavior change.
- Keep remote auth hardening separate from pure file-split PRs.
- Do not combine `core/state.rs` splitting with shared reducer work in one patch.
