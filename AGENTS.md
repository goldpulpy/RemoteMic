# Repository Guidelines

## Project Structure & Module Organization

RemoteMic is a single Rust 2024 binary. `src/main.rs` owns CLI parsing, startup, shutdown, and FIFO writing. `src/audio.rs` manages audio presets and the PulseAudio virtual source; `src/server.rs` contains the Axum HTTP/WebSocket server and session control; `src/page.rs` embeds the HTML, CSS, and JavaScript client; and `src/preflight.rs` checks Linux audio dependencies. Unit tests live beside implementation code in `#[cfg(test)]` modules. CI and release automation are under `.github/workflows/`; build hardening is in `Makefile`.

## Build, Test, and Development Commands

- `cargo build` — compile a native debug binary.
- `cargo run -- --help` — run locally and inspect CLI options.
- `cargo test` — run all unit and async Tokio tests.
- `cargo fmt --all -- --check` — verify rustfmt output without modifying files.
- `cargo clippy --all-targets --all-features -- -D warnings` — run the same strict lint gate used by CI.
- `make release` — build and strip the hardened `x86_64-unknown-linux-musl` binary; requires the musl target/toolchain and `objcopy`.
- `make audit` / `make security` — scan dependencies for advisories or inspect the release binary with `checksec`.

## Coding Style & Naming Conventions

Use standard rustfmt formatting (four-space indentation). Follow Rust naming: `snake_case` for modules, functions, and variables; `PascalCase` for types and traits; `SCREAMING_SNAKE_CASE` for constants. Prefer borrowed inputs, explicit `Result` propagation with `?`, and bounded async channels. Avoid `unwrap` and `expect` in production paths. Keep comments focused on non-obvious constraints; document public behavior with `///` comments.

## Testing Guidelines

Add focused tests next to the changed module using descriptive names such as `parse_quality_rejects_unknown_value`. Use `#[tokio::test]` for asynchronous behavior. Cover success, validation, session lifecycle, and backpressure edge cases. There is no numeric coverage target; every behavior change should include a regression test when practical. Run test, format, and Clippy checks before submitting.

## Commit & Pull Request Guidelines

History follows Conventional Commit prefixes, primarily `feat:` and `fix:`; write imperative, narrowly scoped subjects (for example, `fix: release active session on disconnect`). Pull requests should explain the user-visible effect, implementation approach, and verification commands; link related issues. Include screenshots for UI changes in `src/page.rs`, and note Linux/PulseAudio or PipeWire manual testing when audio behavior changes. Keep `Cargo.lock` committed when dependencies change.
