//! 传输层:同一套 [`crate::Server`] 跑在两种通道上。
//!
//! - [`stdio`]:行分隔 JSON over stdin/stdout。零端口冲突、零防火墙提示,
//!   客户端以子进程方式启动(`latermd --mcp-stdio`)。
//! - [`http`]:`127.0.0.1:<port>`,GUI 进程内常驻 —— **应用开着就能被调**,
//!   这也是「启动应用后让其他 AI 调用」的主通道。

pub mod http;
pub mod stdio;
