# Cavvy

Cavvy is a statically typed, object-oriented programming language compiler written in Rust. It compiles `.cay` source files through this pipeline:

```text
.cay source -> preprocessor -> lexer -> parser -> semantic analysis -> LLVM IR -> native executable
```

The historical project name was EOL, so some CI artifacts and compatibility notes still use `eol`. The canonical source extension is `.cay`.

Current tool version is read from `.verinfo`: `5.1.0-RC.1`.

## Build

The normal compiler build is release mode:

```powershell
cargo build --release
```

The repository expects `llvm-minimal/`, `mingw-minimal/`, and `lib/` to exist locally. If a fresh checkout is missing those directories, run:

```powershell
python setup-llvm.py
```

## Test

Integration tests call the release compiler binaries, so build release first:

```powershell
cargo build --release
cargo test --release --verbose
```

Documentation examples are tested directly from Markdown code fences:

```powershell
.\scripts\test-docs.ps1
```

Cross-platform equivalent:

```bash
python scripts/doc-test.py --build
```

## Documentation

The new documentation site is an mdBook project:

```powershell
cargo install mdbook --locked
mdbook serve
```

Open the generated site at the URL printed by `mdbook serve`. GitHub Actions builds the same book and deploys it to GitHub Pages.

## Main Tools

| Tool | Purpose |
|---|---|
| `cayc` | Compile `.cay` to native executable |
| `cay-check` | Preprocess, lex, parse, and semantically check source |
| `cay-ir` | Emit LLVM IR |
| `ir2exe` | Compile LLVM IR to executable |
| `cay-run` | Compile and run in one command |
| `cay-pre` | Run the preprocessor |
| `cay-rcpl` | Interactive RCPL |
| `cay-lsp` | Language server |
| `cavly` | Package and project manager |
| `cay-bcgen` | Experimental bytecode generator |
| `cay-dt` | Documentation tooling |
| `cay-dp` | Parser/debug preview tooling |

## License

GPL-3.0. See `LICENSE`.
