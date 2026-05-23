# AGENTS.md — Cavvy Compiler Project

## Project identity

Cavvy (Cay) is a statically-typed, object-oriented language compiler written in Rust (2024 edition). It compiles `.cay`/`.eol` source to native machine code via LLVM IR → clang. GPL3 licensed.

- **Real version** (from `.verinfo`): `5.1.0-Alpha.4`. The README says 0.4.8 — it is outdated.
- **Old name**: the project was formerly called "EOL" (Ethernos Object Language). The `.eol` extension and `eol-*` artifact names in CI are legacy holdovers. The canonical extension is `.cay`.

## Build

```bash
cargo build --release     # release is the normal mode (bundled tools are copied only in release)
```

The repo bundles LLVM, MinGW, and link libraries under `llvm-minimal/`, `mingw-minimal/`, `lib/`. These **must exist on disk** before `cargo build` succeeds — `build.rs` copies them to `target/<profile>/`. If they're missing, check `.gitignore` — they may be gitignored and need to be downloaded first via `setup-llvm.py`.

`setup-llvm.py` downloads the LLVM+MinGW bundles from `cavvy-lang/Cavvy-src-Assets` on GitHub. It reads `.verinfo` for version pins. Run it before a fresh clone build when the toolchain dirs are empty.

`.cargo/config.toml` only has linux-musl cross-compile config — ignore it on Windows.

## Test

```bash
# MUST build release first — tests invoke the release compiler binary as a subprocess:
cargo build --release
cargo test --release --verbose
```

Tests live in two places:
- `src/lib.rs` — a few inline `#[cfg(test)]` unit tests (lexer, parser, preprocessor).
- `tests/*.rs` — integration tests that compile `.cay` files under `examples/` with the release-built `cayc`, run the output `.exe`, and assert stdout. **These will all fail** if you haven't built release first.

Test helpers are in `tests/common/mod.rs`: `compile_and_run_eol()`, `compile_eol_expect_error()`, assertion helpers. All integration tests use a global `Mutex` to serialize execution (avoiding file conflicts).

**Leaked temp files**: test runs leave behind `temp_*.exe`, `temp_*.ll`, `temp_*.cay` in `tests/` and `examples/`. These are gitignored but accumulate locally. Don't commit them.

## Binaries (12, not 6 as README claims)

| Binary | Purpose |
|---|---|
| `cayc` | One-stop: `.cay` → `.exe` (invokes clang internally) |
| `cay-ir` | `.cay` → `.ll` (LLVM IR text) |
| `ir2exe` | `.ll` → `.exe` |
| `cay-check` | Syntax + semantic check only (no codegen) |
| `cay-run` | Compile + run in one step |
| `cay-rcpl` | REPL |
| `cay-bcgen` | `.caybc` bytecode generation |
| `cay-lsp` | LSP language server |
| `cavly` | Package manager |
| `cay-dt` | Documentation tool |
| `cay-dp` | Dependency tool |
| `cay-idle` | GUI IDE (egui-based) |
| `cay-pre` | Standalone preprocessor |

All entry points are in `src/bin/*.rs`. They all depend on the library crate (`src/lib.rs`, crate name `cavvy`).

## Architecture

```
.cay source
  → preprocessor (src/preprocessor/) — #include, #define, #ifdef
  → lexer (src/lexer/) — logos-based tokenizer
  → parser (src/parser/) — recursive descent
  → semantic (src/semantic/) — type checking, symbol resolution
  → codegen (src/codegen/) — LLVM IR text generation
  → clang (bundled: llvm-minimal/) — IR → machine code
```

Key modules in `src/`:
- `ast.rs` — all AST node types
- `types.rs` — type system (Type enum, ClassInfo, MethodInfo)
- `error.rs` — `cayError` enum (thiserror), `cayResult<T>`
- `diagnostic.rs` — structured diagnostics (severity, phase, error codes)
- `miette_diagnostic.rs` — pretty-printed miette diagnostics
- `ir/` — IR-related types and utilities
- `preprocessor/` — C-style preprocessor with source maps
- `cavly/` — package manager logic
- `bytecode/` — CayBC bytecode format
- `rcpl/` — REPL logic
- `idle/` — GUI IDE

The lib.rs exposes a `Compiler` struct that wraps the full pipeline. Binaries use it via `use cavvy::Compiler`.

## Error handling conventions

- `cayError` (in `src/error.rs`) is the **sole error type** for the compiler pipeline. Defined with `thiserror::Error`. Every variant carries a `suggestion: String` field for user-facing help text.
- `cayResult<T> = Result<T, cayError>` — use this throughout the compiler.
- `miette` is used for **display only** — converting `cayError` to pretty CLI output. Don't use miette types in the compiler internals.
- `anyhow` is listed as a dependency but is **not used** in the core compiler. Don't introduce it.
- Error reporting functions: `print_miette_error()`, `print_error_with_context()`, `print_tool_error()`, `print_warning()` — all imported from `cavvy::error`.

## Source conventions

- **Comment language**: Chinese and English are mixed freely. Doc comments (`//!`, `///`) are predominantly in Chinese. Implementation comments vary. Don't enforce a single language.
- **No formatter config**: There is no `rustfmt.toml`. Follow the existing style: 4-space indentation, `use crate::` prefixes for internal imports, `use std::` for stdlib.
- **No strict linting**: No `#![deny(...)]` or `#![warn(...)]` crate-level attributes. CI runs with `RUSTFLAGS="-A warnings"` (suppresses all warnings). Don't add deny attributes unless explicitly asked.
- **`use` style**: Module-level imports at the top; function-local `use` only for items used once. `pub use` re-exports in `mod.rs` files for module public API.
- **`.bak` files**: Several modules have `.rs.bak.N` backup files (e.g., `lexer/mod.rs.bak.5`, `parser/statements.rs.bak.4`). These are stale copies. **Never edit them.** If you see them, ignore them.

## CI

- **Nightly build** (`.github/workflows/nb.yml`): Runs daily UTC 02:00 on `windows-latest`. Builds with `stable` toolchain, target `x86_64-pc-windows-gnu`. Names artifacts `eol-*` (legacy). Skips tests if `skip_tests=true` is passed via workflow dispatch.
- **GitHub Pages** (`.github/workflows/jekyll-gh-pages.yml`): Deploys docs from `main` branch. Not relevant to code changes.

## File extension and .gitignore quirks

- `.gitignore` ignores `*.cay` and `*.eol` globally, **except** under `examples/`, `test_docs/`, and `caylibs/`. New `.cay` test files go in `examples/`.
- `*.exe` is globally gitignored (except under `llvm-minimal/bin/`). Built test executables won't be tracked.
- The repo root has many leftover `.exe` files from old test runs. Don't commit them.

## VSCode extension

`vscode-extension/` contains an in-tree VS Code extension with syntax highlighting (`syntaxes/cavvy.tmLanguage.json`) and LSP client. VSIX files are committed directly. If modifying the LSP, check both `src/bin/cay-lsp.rs` and `vscode-extension/src/`.

## Version management

Version numbers live in `.verinfo` (INI-like format). `build.rs` parses this file, combines each version with the git commit hash, and sets `CARGO_*_VERSION` env vars for compile-time embedding. After changing `.verinfo`, `cargo build` will automatically recompile.
