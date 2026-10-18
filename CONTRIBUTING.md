# Contributing to RADIAL

## Setup

1. Install Node.js 22+, Rust stable (1.88+) and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/).
2. `npm install`
3. `npm run tauri dev`

## Before you commit

Run everything a reviewer will run:

```sh
npm run check                                   # tsc --strict, eslint, vitest, cargo test
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
npm run test:ui                                 # Playwright (mock backend)
```

CI runs the same checks on Linux and builds the Windows installers on Windows.

## Conventions

**Rust**
- All conversion logic lives in `crates/radial-core`, which must not depend on Tauri.
- Return `radial_core::Result<T>`; classify failures into an `ErrorKind` the UI can act on. No
  `unwrap()` on user-controlled data.
- Never invoke a shell. External engines take argument lists via `std::process::Command`.
- Every new capability needs: a registry entry with honest options and limitations, a handler that
  follows the plan → temp file → verify → no-clobber persist contract, and integration tests on
  real files in a temp dir.

**TypeScript / React**
- `strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes` are on. Keep them on.
- UI never hard-codes format lists; it renders what `capabilities_for` returns.
- Talk to native code only through `src/lib/backend.ts`.
- Use design tokens from `src/styles/tokens.css`; don't introduce raw colours or durations.
- Every interactive element needs a keyboard path and an accessible name.

**Tests**
- Conversion behaviour is tested in Rust with real encoders. The Playwright suite uses a mock
  backend and verifies interaction only — don't treat it as conversion coverage.
- UI tests write screenshots to `qa/screenshots/ui/`; commit them when the UI changes on purpose.

## Commits and branches

- Small, focused commits with imperative subjects ("Add WebP lossless toggle").
- Update `CHANGELOG.md` (Unreleased) and the docs when behaviour or capabilities change.
- Never commit secrets, `.env` files, signing certificates or personal files.
- No force-pushes or history rewrites on shared branches.
