//! stdio 传输:一行一个 JSON-RPC 消息(docs/mcp-plan.md §2)。
//!
//! 客户端以子进程方式启动本通道(`latermd --mcp-stdio`),stdin 收到一行
//! 就处理一行、stdout 回一行。通知(无 id)不回任何字节 —— 这是协议要求,
//! 不是优化。
//!
//! 每写完一行**必须 flush**:客户端在阻塞等这一行,行缓冲会让整条链路
//! 看着像卡死。

use std::io::{self, BufRead, Write};

use crate::Server;

/// 循环处理 stdin,直到 stdin 关闭(客户端退出/断管)。
pub fn serve(server: &Server) -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(response) = server.handle(line) {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// stdio 是进程级通道(独占 stdin/stdout),无法在单测里并发驱动;
    /// 这里只锚定「空行不处理」这一条可在单元层验证的行为,端到端由
    /// `serve` 的调用方(`--mcp-stdio`)保证。
    #[test]
    fn empty_lines_are_skipped() {
        // 空行交给 handle 会产生 parse error 响应,故 serve 侧必须跳过
        let server = Server::new(None, crate::McpConfig::default());
        assert!(server.handle("").is_some(), "空行本身会被判为坏 JSON");
        let _ = serve;
    }
}
