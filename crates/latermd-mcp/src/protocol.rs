//! JSON-RPC 2.0 的请求/响应结构(docs/mcp-plan.md §2)。
//!
//! 只实现 MCP 需要的三个方法(`initialize` / `tools/list` / `tools/call`)
//! 与 `ping`;`resources`、`prompts`、`sampling` 一概不实现 —— 文档库用
//! tools 表达已经够用,少一层概念也少一处规范漂移面(mcp-plan 风险 #3)。
//!
//! 版本字符串是**固定的**(MCP 2025-06-18),客户端发别的版本也照样应答
//! 自己的版本:能力协商在本实现里只有一个开关集(tools),没有需要按版
//! 本分叉的行为。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// 应答给客户端的协议版本(MCP 2025-06-18)。
pub const PROTOCOL_VERSION: &str = "2025-06-18";
pub const SERVER_NAME: &str = "LaterMD";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// JSON-RPC 标准错误码。
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

/// 入站请求。`id` 为 `None` 的是**通知**(不回响应)。
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

/// 出站响应;`result` 与 `error` 二者必有其一。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Response {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorObject>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ErrorObject {
    pub code: i32,
    pub message: String,
}

impl Response {
    /// 成功响应。
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    /// 错误响应(工具内部错误也走这里:`isError` 是 tools/call 的 result 层,
    /// 协议层错误才有 error 对象)。
    pub fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(ErrorObject {
                code,
                message: message.into(),
            }),
        }
    }

    /// 序列化为一行 JSON(stdio 与 HTTP 都是「一行一个消息」)。
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|error| {
            // 响应结构体里的字段都是可序列化的,失败只可能是 Value 里有
            // 非字符串键之类的病态情况;兜底成协议层错误而非 panic
            serde_json::to_string(&Self::err(Value::Null, INTERNAL_ERROR, error.to_string()))
                .unwrap_or_else(|_| {
                    r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"响应序列化失败"}}"#
                        .to_owned()
                })
        })
    }
}

/// 解析一行入站消息。
pub fn parse_request(line: &str) -> Result<Request, Response> {
    let request: Request = serde_json::from_str(line)
        .map_err(|error| Response::err(Value::Null, PARSE_ERROR, error.to_string()))?;
    if request.jsonrpc != "2.0" {
        return Err(Response::err(
            request.id.unwrap_or(Value::Null),
            INVALID_REQUEST,
            "jsonrpc 字段必须为 \"2.0\"",
        ));
    }
    Ok(request)
}

/// `initialize` 的应答体:能力只有 `tools`。
pub fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 请求解析:标准字段齐全;`params` 与 `id` 缺省也能过(通知)。
    #[test]
    fn parses_request_with_and_without_id() {
        let request = parse_request(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).unwrap();
        assert_eq!(request.id, Some(json!(1)));
        assert_eq!(request.method, "tools/list");
        assert_eq!(request.params, None);

        let notification =
            parse_request(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).unwrap();
        assert_eq!(notification.id, None);
    }

    /// 坏 JSON 与错误版本号都被挡在协议层(id 缺失时回 null)。
    #[test]
    fn malformed_requests_are_rejected() {
        let error = parse_request("not json").unwrap_err();
        assert_eq!(error.error.as_ref().unwrap().code, PARSE_ERROR);

        let error = parse_request(r#"{"jsonrpc":"1.0","id":7,"method":"ping"}"#).unwrap_err();
        assert_eq!(error.error.as_ref().unwrap().code, INVALID_REQUEST);
        assert_eq!(error.id, json!(7), "有 id 时按 id 回错");
    }

    /// 响应行:成功不带 error 字段,失败不带 result 字段(skip_serializing_if)。
    #[test]
    fn response_lines_omit_the_absent_branch() {
        let ok = Response::ok(json!(1), json!({"tools": []})).to_line();
        assert!(!ok.contains("error"), "{ok}");
        assert!(ok.contains("\"result\""), "{ok}");

        let err = Response::err(json!(2), METHOD_NOT_FOUND, "没有这个方法").to_line();
        assert!(!err.contains("result"), "{err}");
        assert!(err.contains("没有这个方法"), "{err}");
    }

    /// 错误码表完整且与 JSON-RPC 规范一致(协议层只用到其中三个,其余
    /// 保留是为了「客户端拿到的码有据可查」)。
    #[test]
    fn error_codes_match_json_rpc_spec() {
        assert_eq!(PARSE_ERROR, -32700);
        assert_eq!(INVALID_REQUEST, -32600);
        assert_eq!(METHOD_NOT_FOUND, -32601);
        assert_eq!(INVALID_PARAMS, -32602);
        assert_eq!(INTERNAL_ERROR, -32603);
    }

    /// initialize 应答带版本与 tools 能力。
    #[test]
    fn initialize_result_carries_version_and_tools_capability() {
        let result = initialize_result();
        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(result["serverInfo"]["name"], SERVER_NAME);
        assert!(result["capabilities"]["tools"].is_object());
    }
}
