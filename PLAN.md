# Plan: B+ → A+

## Phase 1: Clean Compiler Output (quick wins)
_Target: zero warnings, clean clippy_

- [x] Fix all 7 clippy warnings (dead code, derivable impl, field assignment)
- [x] Add `#[allow(dead_code)]` only where methods are intentionally part of future API; remove truly dead code
- [x] Run `cargo clippy --all-targets -- -D warnings` — must pass clean
- [x] Apply auto-fixable clippy suggestions

## Phase 2: Error Handling Hardening
_Target: bulletproof error paths, no silent failures_

- [x] Audit every remaining `.unwrap()` and `.expect()` in non-test code — replace with `?` or graceful handling
- [x] Add `anyhow::Context` to all error-propagation sites that lack context
- [x] Ensure PipeWire reconnection after daemon restart (already implemented with exponential backoff)
- [x] Add error recovery for network disconnections (retry with backoff)
- [x] Config loading: handle corrupt/partial TOML gracefully (fallback to defaults + warn)

## Phase 3: Testing to Production Grade
_Target: 250+ tests, property tests, benchmark coverage_

- [x] Add proptest for graph operations (random node/port/link add/remove sequences)
- [x] Add proptest for audio volume calculations (edge cases: NaN, inf, negative)
- [x] Add proptest for filter matching logic
- [x] Add stress test: 500-node graph operations (verify no quadratic blowup)
- [x] Add benchmark tests for large graph rendering (criterion)
- [x] Add tests for error recovery paths (corrupt config, channel overflow, PW disconnect)
- [x] Add tests for network protocol edge cases (malformed messages, version mismatch)
- [x] Add tests for snapshot save/restore round-trip fidelity
- [x] Test concurrent access patterns on SharedState

## Phase 4: Architecture Refinement
_Target: clean separation, testable controllers_

- [x] Extract command dispatch from PipeflowApp into a standalone `CommandRouter` that can be unit tested without UI
- [x] Create typed error enum for domain layer (replace stringly-typed errors)
- [x] Add protocol version negotiation to network layer (already implemented in ConnectRequest/Response)
- [x] Reduce pub surface area — audit every `pub` item, demote to `pub(crate)` or `pub(super)` where possible
- [x] Standardize method naming: pick verb-first consistently (`select_node` → `add_to_selection` style)

## Phase 5: Performance & Robustness
_Target: handles real-world studio setups without breaking a sweat_

- [x] Add spatial indexing (quadtree or grid) for graph layout collision detection on large graphs
- [x] Profile and optimize the render hot path — ensure <2ms per frame for 200-node graphs
- [x] Pre-allocate meter buffers at stream creation (already implemented with swap buffers)
- [x] Add connection pooling / keepalive for network layer
- [x] Add graceful degradation under load (already implemented — meter updates dropped before control commands)

## Phase 6: Documentation & Polish
_Target: A+ level developer experience_

- [x] Document all public types and functions (rustdoc)
- [x] Add module-level doc comments explaining each layer's responsibility
- [x] Add architecture decision records (ADRs) for key design choices
- [x] Add CONTRIBUTING.md with dev setup, test, and PR guidelines
- [x] Ensure README accurately reflects current feature set and limitations
- [x] Add inline comments for non-obvious algorithms (layout, meter smoothing, safety state machine)

## Success Criteria for A+

- Zero clippy warnings
- Zero unwraps in production code
- 250+ tests including property tests
- Benchmark suite for performance regression detection
- Clean error propagation with user-visible feedback for all failure modes
- Documented public API
- Network protocol versioning
- Handles 500-node graphs without degradation
- PipeWire reconnection works automatically
