//! HTTP 传输:`127.0.0.1` 单连接串行(docs/mcp-plan.md §2/§4)。
//!
//! **为什么只绑回环地址**:默认监听任何端口都是攻击面,而本 server 的
//! 客户端必然是本机上的 AI 进程。bind 到 `127.0.0.1` 已经从系统层挡掉了
//! 外部来源,accept 后再校验一次 `is_loopback` 是双保险(mcp-plan §7)。
//!
//! **为什么串行**:工具调用是毫秒级检索,排队即可;并发模型除了增加
//! 「两个 AI 同时改同一个编辑器缓冲」的风险没有收益 —— 何况本 server 只读。
//!
//! 响应形态按 `Accept` 协商:客户端要 SSE(`text/event-stream`)就回
//! `data: <json>\n\n`,否则回普通 JSON。两个方法的语义完全一致,只是传输
//! 编码不同。

use std::collections::BTreeMap;
use std::io::{self, BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::Server;

/// 单次读写的超时:客户端挂死不该让本线程永久占用一个连接。
const IO_TIMEOUT: Duration = Duration::from_secs(30);
/// 空闲时 accept 的轮询间隔(非阻塞 accept + stop 检查)。
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// 绑到回环地址。`port = 0` 由系统分配端口(测试与「自动选端口」用)。
pub fn bind(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", port))
}

/// 在已绑好的 listener 上循环服务,直到 `stop` 被置位。
pub fn serve_with(listener: TcpListener, server: Server, stop: Arc<AtomicBool>) -> io::Result<()> {
    // 非阻塞 accept:否则 stop 置位后线程还要等下一个连接到来才醒
    listener.set_nonblocking(true)?;
    loop {
        if stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        match listener.accept() {
            Ok((stream, peer)) => {
                if !peer.ip().is_loopback() {
                    // bind 已限定来源,这一层只是防御配置被改动的情况
                    continue;
                }
                if let Err(error) = handle_connection(stream, &server) {
                    eprintln!("LaterMD: MCP 连接处理失败: {error}");
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(error) => return Err(error),
        }
    }
}

/// 绑端口并循环服务(UI 侧入口)。
pub fn serve(server: Server, port: u16, stop: Arc<AtomicBool>) -> io::Result<()> {
    serve_with(bind(port)?, server, stop)
}

/// 处理单个连接:读一个 HTTP 请求 → 交给 server → 写一个响应 → 关闭。
fn handle_connection(mut stream: TcpStream, server: &Server) -> io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let mut reader = io::BufReader::new(stream.try_clone()?);

    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();

    let mut headers: Vec<(String, String)> = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line == "\n" || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_lowercase(), value.trim().to_owned()));
        }
    }
    let headers: BTreeMap<String, String> = headers.into_iter().collect();
    let length: usize = headers
        .get("content-length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0_u8; length];
    if length > 0 {
        reader.read_exact(&mut body)?;
    }
    let body = String::from_utf8_lossy(&body).to_string();

    let response = if method != "POST" {
        // MCP 的方法都是 POST;GET 通常是客户端来开 SSE 流,本实现不支持
        // 长连接流,直接 405 并说明,不假装接住
        (
            405,
            "application/json",
            r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"请用 POST 发送 JSON-RPC 请求"}}"#
                .to_owned(),
        )
    } else if !target.starts_with("/mcp") && target != "/" {
        (
            404,
            "application/json",
            r#"{"error":"not found"}"#.to_owned(),
        )
    } else {
        let payload = server.handle(&body).unwrap_or_default();
        if headers
            .get("accept")
            .is_some_and(|accept| accept.contains("text/event-stream"))
        {
            (200, "text/event-stream", format!("data: {payload}\r\n\r\n"))
        } else {
            (200, "application/json", payload)
        }
    };

    write_response(&mut stream, response.0, response.1, &response.2)
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::McpConfig;

    /// 起一个临时 server,返回(端口, stop 标志, 计数句柄)。
    fn spawn_server(root: Option<std::path::PathBuf>) -> (u16, Arc<AtomicBool>) {
        let listener = bind(0).unwrap();
        let port = listener.local_addr().unwrap().port();
        let stop = Arc::new(AtomicBool::new(false));
        let server = Server::new(root, McpConfig::default());
        let flag = Arc::clone(&stop);
        std::thread::spawn(move || serve_with(listener, server, flag));
        (port, stop)
    }

    /// 发一个请求并返回整个响应报文。
    fn post(port: u16, path: &str, body: &str, accept: Option<&str>) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let accept = accept.unwrap_or("application/json");
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nAccept: {accept}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).unwrap();
        stream.flush().unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    /// 端到端:initialize 走 HTTP 拿到协议版本;响应体是合法 JSON。
    #[test]
    fn http_round_trip_initialize() {
        let (port, stop) = spawn_server(None);
        let response = post(
            port,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
            None,
        );
        stop.store(true, Ordering::SeqCst);
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        let body = response.split("\r\n\r\n").nth(1).unwrap_or_default();
        let value: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(
            value["result"]["protocolVersion"],
            crate::protocol::PROTOCOL_VERSION
        );
    }

    /// Accept 要 SSE 时按 SSE 帧返回(`data: ` 前缀),语义与 JSON 一致。
    #[test]
    fn http_negotiates_sse_when_client_asks() {
        let (port, stop) = spawn_server(None);
        let response = post(
            port,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            Some("text/event-stream"),
        );
        stop.store(true, Ordering::SeqCst);
        let body = response.split("\r\n\r\n").nth(1).unwrap_or_default();
        assert!(body.starts_with("data: "), "{body}");
        assert!(body.contains("search_docs"), "{body}");
    }

    /// 非 POST 与未知路径分别 405 / 404,不假装接住。
    #[test]
    fn non_post_and_unknown_path_are_rejected() {
        let (port, stop) = spawn_server(None);
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .write_all(b"GET /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 405"), "{response}");

        let response = post(port, "/other", "{}", None);
        stop.store(true, Ordering::SeqCst);
        assert!(response.starts_with("HTTP/1.1 404"), "{response}");
    }

    /// 端到端检索:真实文档库 + HTTP 通道 + 真实工具。
    #[test]
    fn http_round_trip_search_docs() {
        let root = std::env::temp_dir().join(format!("latermd-mcp-http-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.md"), "LaterMD 架构决策\n").unwrap();

        let (port, stop) = spawn_server(Some(root.clone()));
        let response = post(
            port,
            "/mcp",
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search_docs","arguments":{"query":"架构"}}}"#,
            None,
        );
        stop.store(true, Ordering::SeqCst);
        let body = response.split("\r\n\r\n").nth(1).unwrap_or_default();
        let value: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(value["result"]["isError"], false, "{body}");
        assert_eq!(
            value["result"]["structuredContent"]["hits"][0]["path"],
            "a.md"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
