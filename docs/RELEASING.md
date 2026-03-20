# Releasing Pipeflow

This repo ships GitHub release artifacts for tagged versions.

## Release checklist

1. Ensure `main` is green locally:
   ```bash
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo test
   cargo build --release
   ```
2. Update `CHANGELOG.md`.
3. Confirm `Cargo.toml` version matches the intended tag.
4. Commit the release-prep changes.
5. Create and push an annotated tag:
   ```bash
   git tag -a v0.1.0 -m "pipeflow v0.1.0"
   git push origin main --follow-tags
   ```
6. Wait for the `Release` GitHub Actions workflow to finish.
7. Verify the GitHub release contains the Linux tarball and generated notes.

## What the release workflow publishes

For tags matching `v*`, GitHub Actions will:

- install the system PipeWire development packages needed for the build
- build `pipeflow` in release mode on `ubuntu-latest`
- bundle these files into `pipeflow-<version>-x86_64-unknown-linux-gnu.tar.gz`
  - `pipeflow`
  - `README.md`
  - `LICENSE-MIT`
  - `LICENSE-APACHE`
  - `assets/pipeflow.desktop`
- create or update the corresponding GitHub release

## Notes

- The binary is Linux-only right now; the workflow intentionally publishes a Linux artifact only.
- `cargo publish` is not part of this flow.
- If you want stricter release gating later, add test/clippy jobs as `needs:` dependencies in the release workflow.
