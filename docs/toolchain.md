# 工具链与构建

## Release 是正常模式

测试和示例运行依赖 `target/release` 下的编译器二进制，因此日常构建使用 release：

```powershell
cargo build --release
```

`build.rs` 会把运行时需要的目录复制到构建目录：

- `llvm-minimal/`
- `mingw-minimal/`
- `lib/`
- `caylibs/`
- `examples/`
- `third-party/`

缺少工具链目录时，先运行：

```powershell
python setup-llvm.py
```

## 测试

```powershell
cargo build --release
cargo test --release --verbose
```

集成测试会编译 `examples/` 下的 `.cay` 文件，再运行生成的可执行文件并断言 stdout。测试使用全局锁串行运行，避免临时文件互相覆盖。

## 文档站

文档站使用 mdBook：

```powershell
cargo install mdbook --locked
mdbook build
mdbook serve
```

输出目录是 `book/`，该目录不提交。
