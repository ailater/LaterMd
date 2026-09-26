//! 本地 MCP server:把 LaterMD 的文档检索能力暴露给外部 AI
//! (docs/mcp-plan.md)。
//!
//! 诉求:坤哥手上有多个 AI 会话,它们反复需要「在这堆 md 里找某段内容」。
//! 与其每个 AI 各自 grep,不如让 LaterMD(它已经有文件树根、搜索服务、
//! 大纲与 Git 快照)把这些能力以 MCP 工具暴露出去,一次接线处处可用。
//!
//! **只读是硬边界**:写入一律经 UI 的用户显式动作(保存走对话框/快捷键、
//! 回滚走确认模态),外部 AI 直接写文件会绕过这道防线,且并发写 + 内存
//! 编辑器缓冲 = 必然丢改。五个工具全部只读,详见 [`tools`] 模块文档。
//!
//! **不依赖 egui/eframe**(铁律二):server 跑在 GUI 的后台线程或 headless
//! 的 `--mcp-stdio` 进程里,与界面生命周期解耦。
//!
//! 两条传输:
//! - [`transport::stdio`]:行分隔 JSON over stdin/stdout,给
//!   `claude mcp add latermd -- latermd --mcp-stdio` 这类以子进程方式启动
//!   的客户端。
//! - [`transport::http`]:`127.0.0.1:<port>`,给 GUI 进程内常驻(应用开着
//!   就能被调)与不支持 stdio 的客户端。

mod protocol;
pub mod tools;
pub mod transport;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub use tools::ToolKind;

/// 配置文件名(平台配置目录,与 `settings.json` / `ai.json` 同级)。
pub const MCP_FILE: &str = "mcp.json";

/// server 配置。**端口与开关不算机密**,但工具开关是权限边界,落盘与 UI
/// 双向一致,不做隐藏(mcp-plan §5)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct McpConfig {
    /// 是否启用(默认关:默认监听任何端口都是攻击面)。
    pub enabled: bool,
    /// HTTP 通道端口(仅本地回环)。
    pub http_port: u16,
    /// 各工具的启用开关(最小权限:只开需要的)。
    pub tools: BTreeMap<String, bool>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            http_port: Self::DEFAULT_PORT,
            tools: ToolKind::ALL
                .iter()
                .map(|kind| (kind.name().to_owned(), true))
                .collect(),
        }
    }
}

impl McpConfig {
    /// 默认端口。
    pub const DEFAULT_PORT: u16 = 8731;
    /// 端口合法区间:低于 1024 是特权端口,不该由桌面应用占。
    pub const PORT_RANGE: std::ops::RangeInclusive<u16> = 1024..=65535;

    /// 某工具是否启用;表里没有的(旧配置缺项)按启用处理。
    pub fn tool_enabled(&self, kind: ToolKind) -> bool {
        self.tools.get(kind.name()).copied().unwrap_or(true)
    }

    /// 归一化:端口钳进合法区间;工具表只保留已知工具名(手改 JSON 可能
    /// 写出旧工具名或拼写错误),缺项补默认。
    pub fn normalize(&mut self) {
        self.http_port = self
            .http_port
            .clamp(*Self::PORT_RANGE.start(), *Self::PORT_RANGE.end());
        let mut tools = BTreeMap::new();
        for kind in ToolKind::ALL {
            tools.insert(
                kind.name().to_owned(),
                self.tools.get(kind.name()).copied().unwrap_or(true),
            );
        }
        self.tools = tools;
    }

    /// 从目录读取;文件缺失或解析失败 = 默认(坏配置不挡启动,与
    /// `ThemeSettings::load` / `AiConfig::load_from` 同口径)。
    pub fn load_from(dir: &Path) -> Self {
        let path = dir.join(MCP_FILE);
        let Ok(bytes) = std::fs::read(&path) else {
            return Self::default();
        };
        let mut config: Self = match serde_json::from_slice(&bytes) {
            Ok(config) => config,
            Err(source) => {
                eprintln!("LaterMD: MCP 配置解析失败,已回落默认: {source}");
                Self::default()
            }
        };
        config.normalize();
        config
    }

    /// 落盘。目录不存在则创建。
    pub fn save_to(&self, dir: &Path) -> Result<(), String> {
        let path = dir.join(MCP_FILE);
        let json = serde_json::to_string_pretty(self)
            .map_err(|source| format!("{}: {source}", path.display()))?;
        std::fs::create_dir_all(dir).map_err(|source| format!("{}: {source}", dir.display()))?;
        std::fs::write(&path, json.as_bytes())
            .map_err(|source| format!("{}: {source}", path.display()))
    }
}

/// 每工具的调用计数(进程内累计,设置页只读展示)。
pub type CallCounts = Arc<Mutex<BTreeMap<String, u64>>>;

/// 文档库根的共享句柄。
///
/// server 跑在后台线程里,而根是 UI 侧(文件树换目录)随时可变的;用共享
/// 句柄而不是把路径拷进线程,换根才不用重启服务。
pub type SharedRoot = Arc<Mutex<Option<PathBuf>>>;

/// Mutex 中毒不致命:拿回内部数据继续用(计数与根都只影响展示/检索范围,
/// 不值得为它崩,更不该把 panic 传进后台线程)。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// MCP server:一次 [`Server::handle`] 处理一行 JSON-RPC 请求。
///
/// 无内部线程、协议层之上没有阻塞 IO —— 传输层怎么循环由 [`transport`]
/// 决定,同一个 `Server` 可被 stdio 与 HTTP 两种通道持有。
#[derive(Debug, Clone)]
pub struct Server {
    root: SharedRoot,
    config: McpConfig,
    counts: CallCounts,
}

impl Server {
    pub fn new(root: Option<PathBuf>, config: McpConfig) -> Self {
        Self::with_root_handle(Arc::new(Mutex::new(root)), config)
    }

    /// 用**外部持有的根句柄**建 server:换根时两侧看到同一个值,后台线程
    /// 不必重启(app 侧持有句柄,文件树换目录即生效)。
    pub fn with_root_handle(root: SharedRoot, config: McpConfig) -> Self {
        Self {
            root,
            config,
            counts: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// 文档库根的共享句柄:UI 侧拿它换根,不必重启服务。
    pub fn shared_root(&self) -> SharedRoot {
        Arc::clone(&self.root)
    }

    /// 换根(文件树换了目录):随后的调用都以新根为界。
    pub fn set_root(&self, root: Option<PathBuf>) {
        *lock(&self.root) = root;
    }

    /// 计数表的共享句柄:UI 侧拿它读累计值,不复制快照。
    pub fn counts(&self) -> CallCounts {
        Arc::clone(&self.counts)
    }

    /// 读计数快照(UI 打开设置页时调;锁粒度是「读一次表」)。
    pub fn snapshot_counts(counts: &CallCounts) -> BTreeMap<String, u64> {
        lock(counts).clone()
    }

    /// 当前文档库根。
    pub fn root(&self) -> Option<PathBuf> {
        lock(&self.root).clone()
    }

    /// 处理一行请求;返回 `None` 表示这是**通知**,按协议不应答。
    pub fn handle(&self, line: &str) -> Option<String> {
        let request = match protocol::parse_request(line) {
            Ok(request) => request,
            Err(response) => return Some(response.to_line()),
        };
        // 通知(无 id)不回响应:MCP 的 notifications/* 全走这条
        let id = request.id.clone()?;
        let params = request.params.clone().unwrap_or_else(|| json!({}));
        let response = match request.method.as_str() {
            "initialize" => protocol::Response::ok(id, protocol::initialize_result()),
            "ping" => protocol::Response::ok(id, json!({})),
            "tools/list" => protocol::Response::ok(id, self.tool_list()),
            "tools/call" => {
                // 缺 `name` 是**请求格式**问题(不是工具执行失败),按协议
                // 走 InvalidParams 的 error 对象;工具本身的失败才用 isError
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Some(
                        protocol::Response::err(
                            id,
                            protocol::INVALID_PARAMS,
                            "tools/call 需要 name 参数",
                        )
                        .to_line(),
                    );
                };
                protocol::Response::ok(id, self.tool_call(name, &params))
            }
            method => protocol::Response::err(
                id,
                protocol::METHOD_NOT_FOUND,
                format!("不支持的方法: {method}"),
            ),
        };
        Some(response.to_line())
    }

    /// `tools/list`:只列**已启用**的工具 —— 关掉的工具对客户端不存在,
    /// 最小权限才真的生效(而不是调了才被拒)。
    fn tool_list(&self) -> Value {
        let tools: Vec<Value> = ToolKind::ALL
            .iter()
            .copied()
            .filter(|kind| self.config.tool_enabled(*kind))
            .map(|kind| {
                json!({
                    "name": kind.name(),
                    "description": kind.description(),
                    "inputSchema": kind.input_schema(),
                })
            })
            .collect();
        json!({ "tools": tools })
    }

    /// `tools/call`:失败也返回**成功响应** + `isError` 内容块(MCP 约定:
    /// 协议层的 error 只用于「请求本身有问题」)。
    fn tool_call(&self, name: &str, params: &Value) -> Value {
        let Some(kind) = ToolKind::from_name(name) else {
            return tool_error(format!("未知工具: {name}"));
        };
        if !self.config.tool_enabled(kind) {
            return tool_error(format!("工具已在设置里关闭: {name}"));
        }
        lock(&self.counts)
            .entry(kind.name().to_owned())
            .and_modify(|count| *count += 1)
            .or_insert(1);
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let context = tools::ToolContext { root: self.root() };
        match tools::call(kind, &context, &arguments) {
            Ok(value) => {
                let text = serde_json::to_string_pretty(&value).unwrap_or_default();
                json!({
                    "content": [{ "type": "text", "text": text }],
                    "structuredContent": value,
                    "isError": false,
                })
            }
            Err(message) => tool_error(message),
        }
    }
}

/// 构造 `isError: true` 的工具结果。
fn tool_error(message: impl Into<String>) -> Value {
    let message = message.into();
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("latermd-mcpcfg-{}-{name}", std::process::id()))
    }

    /// 配置默认态:关闭、默认端口、五工具全开。
    #[test]
    fn default_config_is_disabled_with_all_tools() {
        let config = McpConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.http_port, McpConfig::DEFAULT_PORT);
        for kind in ToolKind::ALL {
            assert!(config.tool_enabled(kind));
        }
    }

    /// 归一化:越界端口被钳,工具表清洗成「已知五项」。
    #[test]
    fn normalize_clamps_port_and_known_tools() {
        let mut config = McpConfig {
            http_port: 80,
            tools: BTreeMap::from([
                ("search_docs".to_owned(), false),
                ("旧工具名".to_owned(), true),
            ]),
            ..McpConfig::default()
        };
        config.normalize();
        assert_eq!(config.http_port, *McpConfig::PORT_RANGE.start());
        assert_eq!(config.tools.len(), ToolKind::ALL.len(), "未知项被丢弃");
        assert!(!config.tool_enabled(ToolKind::SearchDocs));
        assert!(config.tool_enabled(ToolKind::ReadDocument), "缺项补默认");
    }

    /// 落盘往返(含工具开关与端口)。
    #[test]
    fn save_load_round_trip() {
        let dir = temp_dir("roundtrip");
        let mut config = McpConfig {
            enabled: true,
            http_port: 9000,
            ..McpConfig::default()
        };
        config
            .tools
            .insert(ToolKind::GitStatus.name().to_owned(), false);
        config.save_to(&dir).unwrap();
        let loaded = McpConfig::load_from(&dir);
        assert_eq!(loaded, config);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 坏 JSON 回落默认且不 panic。
    #[test]
    fn corrupt_json_falls_back_to_default() {
        let dir = temp_dir("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(MCP_FILE), b"{oops").unwrap();
        assert_eq!(McpConfig::load_from(&dir), McpConfig::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn call(server: &Server, method: &str, params: Value) -> Value {
        let line =
            json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
        let response = server.handle(&line).expect("有 id 必有响应");
        serde_json::from_str::<Value>(&response).unwrap()
    }

    /// initialize 应答带协议版本与 tools 能力。
    #[test]
    fn initialize_answers_version_and_capabilities() {
        let server = Server::new(None, McpConfig::default());
        let response = call(&server, "initialize", json!({}));
        assert_eq!(
            response["result"]["protocolVersion"],
            protocol::PROTOCOL_VERSION
        );
        assert!(response["result"]["capabilities"]["tools"].is_object());
    }

    /// 通知(无 id)不产生响应行。
    #[test]
    fn notifications_get_no_response() {
        let server = Server::new(None, McpConfig::default());
        assert_eq!(
            server.handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
            None
        );
    }

    /// 坏 JSON 也回一行协议层错误(客户端不会等到超时)。
    #[test]
    fn malformed_line_yields_parse_error() {
        let server = Server::new(None, McpConfig::default());
        let response = server.handle("not json").unwrap();
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["error"]["code"], protocol::PARSE_ERROR);
    }

    /// 未知方法走 METHOD_NOT_FOUND。
    #[test]
    fn unknown_method_is_reported() {
        let server = Server::new(None, McpConfig::default());
        let response = call(&server, "resources/list", json!({}));
        assert_eq!(response["error"]["code"], protocol::METHOD_NOT_FOUND);
    }

    /// tools/list 只列启用项;关掉一个就少一个。
    #[test]
    fn tool_list_hides_disabled_tools() {
        let mut config = McpConfig::default();
        config
            .tools
            .insert(ToolKind::GitStatus.name().to_owned(), false);
        let server = Server::new(None, config);
        let response = call(&server, "tools/list", json!({}));
        let tools = response["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), ToolKind::ALL.len() - 1);
        assert!(!tools.iter().any(|tool| tool["name"] == "git_status"));
    }

    /// 共享根句柄:一侧换根,另一侧立即看到(文件树换目录不必重启服务)。
    #[test]
    fn shared_root_is_visible_to_both_sides() {
        let server = Server::new(None, McpConfig::default());
        let handle = server.shared_root();
        assert_eq!(server.root(), None);
        server.set_root(Some(PathBuf::from("/vault")));
        assert_eq!(
            handle.lock().unwrap().clone(),
            Some(PathBuf::from("/vault"))
        );

        // 用同一句柄建的另一个 server 也看到新根(后台线程与 UI 的共享)
        let mirror = Server::with_root_handle(Arc::clone(&handle), McpConfig::default());
        assert_eq!(mirror.root(), Some(PathBuf::from("/vault")));
    }

    /// tools/call:未设根时返回 isError 内容块,且计数照涨(调用确实发生了)。
    #[test]
    fn tool_call_reports_error_and_counts() {
        let server = Server::new(None, McpConfig::default());
        let response = call(
            &server,
            "tools/call",
            json!({ "name": "search_docs", "arguments": { "query": "x" } }),
        );
        let result = &response["result"];
        assert_eq!(result["isError"], true, "{response}");
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("未设置文件树根目录"));
        assert_eq!(
            Server::snapshot_counts(&server.counts()).get("search_docs"),
            Some(&1)
        );
    }

    /// tools/call:关掉的工具拒绝执行且不计数(未启用即不可用)。
    #[test]
    fn disabled_tool_is_refused_without_counting() {
        let mut config = McpConfig::default();
        config
            .tools
            .insert(ToolKind::SearchDocs.name().to_owned(), false);
        let server = Server::new(None, config);
        let response = call(&server, "tools/call", json!({ "name": "search_docs" }));
        assert_eq!(response["result"]["isError"], true);
        assert!(response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("关闭"));
        assert!(Server::snapshot_counts(&server.counts()).is_empty());
    }

    /// tools/call:未知工具名走 isError(工具层失败);缺 name 走协议层的
    /// InvalidParams(请求格式问题),两者都不 panic。
    #[test]
    fn unknown_tool_and_missing_name_are_reported() {
        let server = Server::new(None, McpConfig::default());
        let response = call(&server, "tools/call", json!({ "name": "delete_file" }));
        assert_eq!(response["result"]["isError"], true);

        let response = call(&server, "tools/call", json!({}));
        assert_eq!(response["error"]["code"], protocol::INVALID_PARAMS);
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("name"));
    }

    /// 端到端:真实文档库上的一次 search_docs(结构化内容随结果返回)。
    #[test]
    fn tool_call_searches_a_real_vault() {
        let root = temp_dir("e2e");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.md"), "LaterMD 架构决策\n").unwrap();
        let server = Server::new(Some(root.clone()), McpConfig::default());
        let response = call(
            &server,
            "tools/call",
            json!({ "name": "search_docs", "arguments": { "query": "架构" } }),
        );
        let result = &response["result"];
        assert_eq!(result["isError"], false, "{response}");
        assert_eq!(result["structuredContent"]["hits"][0]["path"], "a.md");
        assert_eq!(result["structuredContent"]["hits"][0]["line_no"], 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
