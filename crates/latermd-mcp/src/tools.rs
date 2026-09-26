//! 五个只读工具的定义与执行(docs/mcp-plan.md §4)。
//!
//! **只读是硬边界**:写入一律经 UI 的用户显式动作(保存要走对话框/快捷
//! 键、回滚要走确认模态),让外部 AI 直接写文件等于绕过这道防线,且 AI
//! 并发写 + 编辑器缓冲在内存 = 必然丢改。因此这里没有、也不会有写工具;
//! 需要写能力时单开 ADR(mcp-plan 风险 #5)。
//!
//! **路径安全**:所有 `path` 参数先 `canonicalize` 再校验前缀落在文件树
//! 根之内 —— `..`、符号链接逃逸、绝对路径越界一律拒绝。根未设置时全部
//! 工具统一报「未设置文件树根目录」(与 AI commit message 的既有口径一致,
//! decisions-pending #14)。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// 工具种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ToolKind {
    /// 全文检索。
    SearchDocs,
    /// 读取文档原文(支持分片)。
    ReadDocument,
    /// 文档大纲。
    Outline,
    /// 列目录(尊重 .gitignore)。
    ListFiles,
    /// Git 改动列表(只读)。
    GitStatus,
}

impl ToolKind {
    /// MCP 工具名(客户端按此调用)。
    pub fn name(self) -> &'static str {
        match self {
            Self::SearchDocs => "search_docs",
            Self::ReadDocument => "read_document",
            Self::Outline => "outline",
            Self::ListFiles => "list_files",
            Self::GitStatus => "git_status",
        }
    }

    /// 一句话说明(进 `tools/list`)。
    pub fn description(self) -> &'static str {
        match self {
            Self::SearchDocs => "在文档库中全文检索,返回路径、行号与命中行文本",
            Self::ReadDocument => "读取单个文档的内容,支持按行分片",
            Self::Outline => "读取单个文档的大纲(标题层级与行号)",
            Self::ListFiles => "列出文档库内的目录与文件,尊重 .gitignore",
            Self::GitStatus => "列出当前 Git 仓库的未提交改动(只读)",
        }
    }

    /// 全部工具(`tools/list` 的顺序,也是设置页开关的顺序)。
    pub const ALL: [ToolKind; 5] = [
        Self::SearchDocs,
        Self::ReadDocument,
        Self::Outline,
        Self::ListFiles,
        Self::GitStatus,
    ];

    /// 按名字反查;未知名字返回 `None`(客户端打错工具名走 MethodNotFound
    /// 之外的「工具不存在」分支,更贴切)。
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.name() == name)
    }

    /// 入参 JSON Schema(`tools/list` 的 `inputSchema`)。
    pub fn input_schema(self) -> Value {
        match self {
            Self::SearchDocs => json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "检索词,按正则解释" },
                    "case_insensitive": { "type": "boolean", "description": "是否忽略大小写,默认 false" },
                    "max_hits": { "type": "integer", "description": "命中上限,默认 500" }
                },
                "required": ["query"]
            }),
            Self::ReadDocument => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "相对文档库根的文件路径" },
                    "offset": { "type": "integer", "description": "起始行(0 起),默认 0" },
                    "limit": { "type": "integer", "description": "行数,默认 200" }
                },
                "required": ["path"]
            }),
            Self::Outline => json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "相对文档库根的文件路径" }
                },
                "required": ["path"]
            }),
            Self::ListFiles => json!({
                "type": "object",
                "properties": {
                    "dir": { "type": "string", "description": "子目录,默认库根" },
                    "glob": { "type": "string", "description": "gitignore 风格的 glob 过滤" },
                    "limit": { "type": "integer", "description": "条目上限,默认 500" }
                }
            }),
            Self::GitStatus => json!({ "type": "object", "properties": {} }),
        }
    }
}

/// 工具执行的上下文:文档库根(文件树根)。`None` = 用户还没选根。
#[derive(Debug, Clone, Default)]
pub struct ToolContext {
    pub root: Option<PathBuf>,
}

/// 取根;未设置时统一文案(与 decisions-pending #14 同口径)。
fn require_root(ctx: &ToolContext) -> Result<&PathBuf, String> {
    ctx.root
        .as_ref()
        .ok_or_else(|| "未设置文件树根目录:请先在 LaterMD 里选择文档库根目录".to_owned())
}

/// 把入参路径解析成**库内的绝对路径**。
///
/// 相对路径按库根拼,绝对路径原样取;随后 `canonicalize` 消掉 `..` 与符号
/// 链接,再校验前缀落在库根内 —— 越界一律拒绝,不返回任何库外内容。
fn resolve_path(root: &Path, raw: &str) -> Result<PathBuf, String> {
    if raw.trim().is_empty() {
        return Err("path 不能为空".to_owned());
    }
    let candidate = Path::new(raw);
    let candidate = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    let canonical = candidate
        .canonicalize()
        .map_err(|_| format!("路径不存在或不可读: {raw}"))?;
    let root_canonical = root
        .canonicalize()
        .map_err(|_| "文档库根目录不可读".to_owned())?;
    if !canonical.starts_with(&root_canonical) {
        return Err(format!("路径越出文档库根: {raw}"));
    }
    Ok(canonical)
}

/// POSIX 分隔符的相对路径串(跨平台输出一致,客户端可原样回传给下一个工具)。
fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 取可选整数参数:缺失用默认,超出合法区间钳住(不因客户端乱填而拒绝)。
fn opt_u64(params: &Value, key: &str, default: u64, max: u64) -> Result<u64, String> {
    let value = match params.get(key) {
        None | Some(Value::Null) => return Ok(default),
        Some(value) => value,
    };
    let number = value
        .as_u64()
        .ok_or_else(|| format!("{key} 必须是非负整数"))?;
    Ok(number.clamp(0, max))
}

/// 取可选字符串参数。
fn opt_str<'a>(params: &'a Value, key: &str) -> Result<Option<&'a str>, String> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or_else(|| format!("{key} 必须是字符串")),
    }
}

/// 读取文本文件:非 UTF-8 与超限文件都明确报错,不静默返回半截内容。
fn read_text(path: &Path) -> Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|error| format!("读取失败: {error}"))?;
    if metadata.len() > latermd_search::MAX_FILE_BYTES {
        return Err(format!(
            "文件超过 {} MB,请用 read_document 的分片参数或换一个文件",
            latermd_search::MAX_FILE_BYTES / 1024 / 1024
        ));
    }
    std::fs::read_to_string(path).map_err(|error| format!("读取失败: {error}"))
}

/// 执行工具;返回给客户端的结构化结果(由调用方包成 MCP 的 content)。
pub fn call(kind: ToolKind, ctx: &ToolContext, params: &Value) -> Result<Value, String> {
    let params = if params.is_null() {
        json!({})
    } else {
        params.clone()
    };
    match kind {
        ToolKind::SearchDocs => search_docs(ctx, &params),
        ToolKind::ReadDocument => read_document(ctx, &params),
        ToolKind::Outline => outline(ctx, &params),
        ToolKind::ListFiles => list_files(ctx, &params),
        ToolKind::GitStatus => git_status(ctx),
    }
}

fn search_docs(ctx: &ToolContext, params: &Value) -> Result<Value, String> {
    let root = require_root(ctx)?;
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .ok_or_else(|| "query 必填".to_owned())?;
    let case_insensitive = params
        .get("case_insensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let max_hits = opt_u64(params, "max_hits", latermd_search::MAX_HITS as u64, 5_000)? as usize;
    let outcome = latermd_search::search_sync(
        &latermd_search::SearchQuery {
            root: root.clone(),
            pattern: query.to_owned(),
            case_insensitive,
        },
        max_hits.max(1),
    )
    .map_err(|error| error.to_string())?;
    let hits: Vec<Value> = outcome
        .hits
        .iter()
        .map(|hit| {
            json!({
                "path": relative_display(root, &hit.path),
                "line_no": hit.line_no,
                "line_text": hit.line_text,
            })
        })
        .collect();
    Ok(json!({ "hits": hits, "truncated": outcome.truncated }))
}

fn read_document(ctx: &ToolContext, params: &Value) -> Result<Value, String> {
    let root = require_root(ctx)?;
    let raw = params
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "path 必填".to_owned())?;
    let path = resolve_path(root, raw)?;
    let text = read_text(&path)?;
    let lines: Vec<&str> = text.lines().collect();
    let offset = opt_u64(params, "offset", 0, u64::MAX)? as usize;
    let limit = opt_u64(params, "limit", 200, 5_000)? as usize;
    let start = offset.min(lines.len());
    let end = (start + limit.max(1)).min(lines.len());
    Ok(json!({
        "path": relative_display(root, &path),
        "text": lines[start..end].join("\n"),
        "total_lines": lines.len(),
        "offset": start,
        "truncated": end < lines.len(),
    }))
}

fn outline(ctx: &ToolContext, params: &Value) -> Result<Value, String> {
    let root = require_root(ctx)?;
    let raw = params
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "path 必填".to_owned())?;
    let path = resolve_path(root, raw)?;
    let text = read_text(&path)?;
    let items: Vec<Value> = latermd_md::outline(&text)
        .iter()
        .map(|item| {
            // span 是字节区间,换算成 1 起行号才对客户端有用。标题的 span
            // 会吸收上一块尾部的换行(latermd-md 的已知行为),故起点落在
            // `\n` 上时要把它算进前一行、再进一行
            let start = item.span.start.min(text.len());
            let leading_newlines = text[start..].chars().take_while(|ch| *ch == '\n').count();
            let line_no = text[..start].matches('\n').count() + 1 + leading_newlines;
            json!({ "level": item.level, "text": item.text, "line_no": line_no })
        })
        .collect();
    Ok(json!({ "path": relative_display(root, &path), "outline": items }))
}

fn list_files(ctx: &ToolContext, params: &Value) -> Result<Value, String> {
    let root = require_root(ctx)?;
    let dir = opt_str(params, "dir")?.map(PathBuf::from);
    let glob = opt_str(params, "glob")?;
    let limit = opt_u64(
        params,
        "limit",
        latermd_search::MAX_LIST_ENTRIES as u64,
        5_000,
    )? as usize;
    // 子目录同样先过越界校验(否则 `dir=../..` 能把库外目录列出来)
    let sub = match &dir {
        Some(dir) => Some(
            resolve_path(root, &dir.to_string_lossy())?
                .strip_prefix(root.canonicalize().unwrap_or_else(|_| root.clone()))
                .unwrap_or(dir)
                .to_path_buf(),
        ),
        None => None,
    };
    let outcome = latermd_search::list_files(root, sub.as_deref(), glob, limit.max(1))?;
    let entries: Vec<Value> = outcome
        .entries
        .iter()
        .map(|entry| {
            json!({
                "path": entry.path.to_string_lossy().replace('\\', "/"),
                "is_dir": entry.is_dir,
            })
        })
        .collect();
    Ok(json!({ "entries": entries, "truncated": outcome.truncated }))
}

fn git_status(ctx: &ToolContext) -> Result<Value, String> {
    let root = require_root(ctx)?;
    // 从库根向上探测仓库(UI 侧同一口径,decisions-pending #17):文件树根
    // 常是仓库子目录
    let repo = latermd_git::discover(root).map_err(|_| "当前文档库不在 Git 仓库内".to_owned())?;
    let snapshot = latermd_git::status(&repo, latermd_git::DEFAULT_STATUS_LIMIT)
        .map_err(|error| format!("读取 Git 状态失败: {error}"))?;
    let entries: Vec<Value> = snapshot
        .entries
        .iter()
        .map(|entry| json!({ "path": entry.path, "code": entry.code.to_string() }))
        .collect();
    Ok(json!({ "entries": entries, "truncated": snapshot.truncated }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造样本库:两个 md + 一个被 .gitignore 排除的目录 + 非 md 文件。
    fn sample_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("latermd-mcp-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("notes")).unwrap();
        std::fs::write(
            root.join("a.md"),
            "# 标题一\n正文 LaterMD\n## 子标题\n架构决策\n",
        )
        .unwrap();
        std::fs::write(root.join("notes/b.md"), "# 嵌套\nlatermd 小写\n").unwrap();
        std::fs::write(root.join(".gitignore"), "drafts/\n").unwrap();
        std::fs::create_dir(root.join("drafts")).unwrap();
        std::fs::write(root.join("drafts/hidden.md"), "latermd 草稿\n").unwrap();
        root
    }

    fn ctx(root: &Path) -> ToolContext {
        ToolContext {
            root: Some(root.to_path_buf()),
        }
    }

    /// 工具名与 schema 一一对应,且名字可反查(客户端按名字调用)。
    #[test]
    fn names_round_trip_and_all_have_schemas() {
        for kind in ToolKind::ALL {
            assert_eq!(ToolKind::from_name(kind.name()), Some(kind));
            assert_eq!(kind.input_schema()["type"], "object");
            assert!(!kind.description().is_empty());
        }
        assert_eq!(ToolKind::from_name("nope"), None);
    }

    /// 未设根:五个工具统一报「未设置文件树根目录」,不 panic 也不泄漏。
    #[test]
    fn every_tool_reports_missing_root() {
        for kind in ToolKind::ALL {
            let error = call(kind, &ToolContext::default(), &json!({})).unwrap_err();
            assert!(error.contains("未设置文件树根目录"), "{kind:?}: {error}");
        }
    }

    /// search_docs:命中带相对路径与行号;.gitignore 排除项不出现。
    #[test]
    fn search_docs_returns_relative_paths_and_line_numbers() {
        let root = sample_root("search");
        let result = call(
            ToolKind::SearchDocs,
            &ctx(&root),
            &json!({ "query": "架构", "case_insensitive": true }),
        )
        .unwrap();
        let hits = result["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1, "{result}");
        assert_eq!(hits[0]["path"], "a.md");
        assert_eq!(hits[0]["line_no"], 4);
        assert!(!result["truncated"].as_bool().unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// read_document:分片语义(offset/limit)与总行数、截断标志。
    #[test]
    fn read_document_slices_lines() {
        let root = sample_root("read");
        let all = call(
            ToolKind::ReadDocument,
            &ctx(&root),
            &json!({"path": "a.md"}),
        )
        .unwrap();
        assert_eq!(all["total_lines"], 4);
        assert!(!all["truncated"].as_bool().unwrap());

        let slice = call(
            ToolKind::ReadDocument,
            &ctx(&root),
            &json!({ "path": "a.md", "offset": 1, "limit": 2 }),
        )
        .unwrap();
        assert_eq!(slice["text"], "正文 LaterMD\n## 子标题");
        assert_eq!(slice["offset"], 1);
        assert!(slice["truncated"].as_bool().unwrap());

        // 越界 offset 被钳到末尾,不 panic
        let beyond = call(
            ToolKind::ReadDocument,
            &ctx(&root),
            &json!({ "path": "a.md", "offset": 99 }),
        )
        .unwrap();
        assert_eq!(beyond["text"], "");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// outline:标题层级与行号(字节 span 换算)。
    #[test]
    fn outline_reports_levels_and_line_numbers() {
        let root = sample_root("outline");
        let result = call(ToolKind::Outline, &ctx(&root), &json!({"path": "a.md"})).unwrap();
        let items = result["outline"].as_array().unwrap();
        assert_eq!(items.len(), 2, "{result}");
        assert_eq!(items[0]["level"], 1);
        assert_eq!(items[0]["text"], "标题一");
        assert_eq!(items[0]["line_no"], 1);
        assert_eq!(items[1]["level"], 2);
        assert_eq!(items[1]["line_no"], 3);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// list_files:gitignore 生效、目录与文件都报;glob 过滤可用。
    #[test]
    fn list_files_honors_gitignore_and_glob() {
        let root = sample_root("list");
        let result = call(ToolKind::ListFiles, &ctx(&root), &json!({})).unwrap();
        let paths: Vec<&str> = result["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["path"].as_str().unwrap())
            .collect();
        assert!(paths.contains(&"a.md"), "{paths:?}");
        assert!(paths.contains(&"notes"));
        assert!(!paths.iter().any(|path| path.starts_with("drafts")));

        let globbed = call(
            ToolKind::ListFiles,
            &ctx(&root),
            &json!({ "glob": "notes/*.md" }),
        )
        .unwrap();
        let globbed_paths: Vec<&str> = globbed["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["path"].as_str().unwrap())
            .collect();
        assert_eq!(globbed_paths, vec!["notes/b.md"], "{globbed_paths:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// git_status:非 Git 目录报「不在仓库内」,不 panic。
    #[test]
    fn git_status_reports_when_not_a_repo() {
        let root = sample_root("git");
        let error = call(ToolKind::GitStatus, &ctx(&root), &json!({})).unwrap_err();
        assert!(error.contains("Git"), "{error}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 路径安全:`..` 逃逸、绝对路径越界、空路径一律拒绝。
    #[test]
    fn path_escape_is_rejected() {
        let root = sample_root("escape");
        for path in ["../outside.md", "/etc/passwd", "", "   "] {
            let error = call(
                ToolKind::ReadDocument,
                &ctx(&root),
                &json!({ "path": path }),
            )
            .unwrap_err();
            assert!(
                error.contains("越出") || error.contains("不能为空") || error.contains("不存在"),
                "{path} => {error}"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 缺必填参数报参数错误,不 panic。
    #[test]
    fn missing_required_params_are_reported() {
        let root = sample_root("required");
        assert!(call(ToolKind::SearchDocs, &ctx(&root), &json!({})).is_err());
        assert!(call(ToolKind::ReadDocument, &ctx(&root), &json!({})).is_err());
        assert!(call(ToolKind::Outline, &ctx(&root), &json!({})).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
