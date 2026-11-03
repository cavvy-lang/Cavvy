//! `cay-lsp` 可执行文件入口。
//!
//! 具体协议实现位于 `cavvy::lsp`；本文件只处理 `--help` / `--version`，然后
//! 通过 stdio 启动语言服务器。

use std::env;

use cavvy::lsp::server::run_stdio;

const VERSION: &str = env!("CAY_LSP_VERSION");

#[tokio::main]
async fn main() {
    let _ = env_logger::try_init();
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && (args[1] == "--version" || args[1] == "-v") {
        println!("cay-lsp {}", VERSION);
        return;
    }

    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        println!("Cavvy Language Server Protocol (LSP) implementation");
        println!();
        println!("Usage: cay-lsp [OPTIONS]");
        println!();
        println!("Options:");
        println!("  -h, --help       Print help");
        println!("  -v, --version    Print version");
        println!();
        println!("This LSP server communicates via stdin/stdout using JSON-RPC protocol.");
        println!();
        println!("Features:");
        println!("  - diagnostics (lexer / parser / semantic)");
        println!("  - completion (members, locals, workspace, builtins)");
        println!("  - hover with signatures and documentation");
        println!("  - go-to-definition / references / rename");
        println!("  - signature help");
        println!("  - document & workspace symbols");
        println!("  - semantic tokens / folding / inlay hints / document links");
        println!("  - quick fixes for unknown members");
        println!();
        println!("Built-in declarations are loaded from caylibs/__builtins__ (see its README).");
        return;
    }

    run_stdio().await;
}
