# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-03-14

Initial public release.

### Added
- Interactive PipeWire graph editing with pan, zoom, drag, multi-select, and link management.
- Per-node and per-channel audio control with mute, live metering, and detailed node inspection.
- Saved setup snapshots, undo/redo, group management, and command palette workflows.
- Safety modes for normal, read-only, and stage-safe operation.
- Headless gRPC server mode and remote-control workflow over SSH tunneling.
- Built-in help, project documentation, desktop entry metadata, and CI.

### Changed
- Refined navigation, toolbar safety presence, saved-setup wording, and general UX copy.
- Improved visual hierarchy, empty states, operation feedback, and remote-mode trust cues.

### Fixed
- Cleaned up clippy warnings and hardened error handling paths.
- Improved selection behavior, inspector visibility, auto-layout handling, and volume warning recovery.

[Unreleased]: https://github.com/trusch/pipeflow/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/trusch/pipeflow/releases/tag/v0.1.0
