//! 图床配置与上传(docs/image-plan.md C 段)。
//!
//! 定位:**纯配置 + 上传契约**,不 import egui(铁律 2,业务逻辑不依赖 UI
//! 框架)。token 不在本 crate 的任何落盘物里 —— [`BedProfile`] 序列化后只有
//! 连接参数,凭据由调用方(latermd-app)经 latermd-creds 按
//! `service=latermd-bed / account=profile id` 存取,上传时以参数传入
//! ([`BedUploader::upload`]),本 crate 不读钥匙串。
//!
//! ## 纯函数分层
//!
//! [`plan_upload`] 把一次上传折成 [`UploadPlan`](方法/URL/头/体,全部已替换
//! `${TOKEN}` / `${NAME}` 占位),网络只发生在 [`http::UreqUploader`] 执行
//! 计划这一步 —— 计划可以穷尽单测,不需要真实网络(不做造假网络测试,与
//! latermd-ai 同口径)。
//!
//! ## 依赖纪律(image-plan C 段决策)
//!
//! * HTTP 复用依赖树里的 ureq 3.x 并开 `multipart` feature,**不引 reqwest**
//!   (避免第二套 HTTP 栈)。
//! * 返回 JSON 的 URL 抽取用手写点分路径([`extract_url`]),**不引 jsonpath**。
//! * 不做各家对象存储 SDK:预置 SM.MS / GitHub(Contents API,base64)/
//!   自定义三张模板,其余让用户走自定义表单。

mod http;

pub use http::{BedUploader, UreqUploader};

use serde::{Deserialize, Serialize};

/// token 占位符:出现在 header 值或 api_url 里,运行时替换为钥匙串里的
/// 真值。占位符本身可以随便给人看(写进 beds.json 不泄密),真值绝不落盘。
pub const TOKEN_PLACEHOLDER: &str = "${TOKEN}";

/// 文件名占位符:出现在 api_url 里(GitHub Contents API 的路径含文件名),
/// 运行时替换为 percent-encode 后的本次文件名。
pub const NAME_PLACEHOLDER: &str = "${NAME}";

/// 上传体的编码方式。方法由它一并决定:multipart → POST,base64 JSON → PUT
/// (GitHub Contents API 的约定)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BedBody {
    /// `multipart/form-data` 表单(SM.MS / Lsky / 多数自建图床)。
    #[default]
    Multipart,
    /// base64 JSON(GitHub Contents API:PUT,`{"message","content"}`)。
    Base64Json,
}

impl BedBody {
    /// 设置页显示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Multipart => "multipart 表单",
            Self::Base64Json => "base64 JSON(GitHub)",
        }
    }
}

/// 一个图床配置。serde 落 `beds.json`(与 settings.json 同目录);**结构里
/// 没有 token 字段** —— 凭据走系统钥匙串,account 就是 `id`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BedProfile {
    /// 唯一 id(新增时由 app 侧分配),也是 creds 的 account 键。预置模板
    /// 携带空 id,落盘前必须已分配。
    pub id: String,
    /// 用户可见名。
    pub name: String,
    /// 上传端点;可含 [`NAME_PLACEHOLDER`],运行时替换为本次文件名。
    pub api_url: String,
    /// multipart 表单字段名(SM.MS=`smfile`,Lsky=`file`;base64 JSON 不用)。
    pub file_field: String,
    /// 附加请求头;值里可写 [`TOKEN_PLACEHOLDER`]。
    pub headers: Vec<(String, String)>,
    /// 返回 JSON 里取 URL 的**点分路径**,如 `data.url`、`content.download_url`。
    pub url_path: String,
    /// 有的图床返回路径而非完整 URL,此前缀拼在前面;`None` = 原样使用。
    pub url_prefix: Option<String>,
    /// 上传体编码(同时决定 HTTP 方法),见 [`BedBody`]。
    pub body: BedBody,
}

impl BedProfile {
    /// SM.MS 模板(v2 API,multipart 字段 `smfile`,token 裸放 Authorization)。
    pub fn preset_smms() -> Self {
        Self {
            id: String::new(),
            name: "SM.MS".to_owned(),
            api_url: "https://sm.ms/api/v2/upload".to_owned(),
            file_field: "smfile".to_owned(),
            headers: vec![("Authorization".to_owned(), TOKEN_PLACEHOLDER.to_owned())],
            url_path: "data.url".to_owned(),
            url_prefix: None,
            body: BedBody::Multipart,
        }
    }

    /// GitHub Contents API 模板:PUT base64,文件路径在 URL 里(含
    /// [`NAME_PLACEHOLDER`]),返回的 `content.download_url` 是完整 URL。
    /// `OWNER/REPO` 由用户改。
    pub fn preset_github() -> Self {
        Self {
            id: String::new(),
            name: "GitHub".to_owned(),
            api_url: "https://api.github.com/repos/OWNER/REPO/contents/images/${NAME}".to_owned(),
            file_field: "content".to_owned(),
            headers: vec![
                (
                    "Authorization".to_owned(),
                    format!("Bearer {TOKEN_PLACEHOLDER}"),
                ),
                (
                    "Accept".to_owned(),
                    "application/vnd.github+json".to_owned(),
                ),
            ],
            url_path: "content.download_url".to_owned(),
            url_prefix: None,
            body: BedBody::Base64Json,
        }
    }

    /// 自定义模板:最常见的 Lsky 系口径打底,逐项改。
    pub fn preset_custom() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            api_url: String::new(),
            file_field: "file".to_owned(),
            headers: vec![(
                "Authorization".to_owned(),
                format!("Bearer {TOKEN_PLACEHOLDER}"),
            )],
            url_path: "data.url".to_owned(),
            url_prefix: None,
            body: BedBody::Multipart,
        }
    }

    /// 归一化:各字段 trim,`url_prefix` 空串折成 `None`,丢掉名字为空的
    /// 请求头(手编 beds.json 的常见脏数据)。
    pub fn normalize(&mut self) {
        self.name = self.name.trim().to_owned();
        self.api_url = self.api_url.trim().to_owned();
        self.file_field = self.file_field.trim().to_owned();
        self.url_path = self.url_path.trim().to_owned();
        self.url_prefix = self
            .url_prefix
            .as_deref()
            .map(str::trim)
            .filter(|prefix| !prefix.is_empty())
            .map(str::to_owned);
        self.headers.retain(|(name, _)| !name.trim().is_empty());
        self.headers = self
            .headers
            .iter()
            .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
            .collect();
    }

    /// 状态栏 / 图片框下拉的显示名:未命名时退 API 主机名,再退「(未命名)」。
    pub fn display_name(&self) -> &str {
        if !self.name.is_empty() {
            &self.name
        } else {
            "(未命名)"
        }
    }
}

/// 图床上传错误。文案面向用户(中文),可直接上提示行;任何变体都不含
/// token 真值(执行层有最后一道消毒,见 [`http::UreqUploader`])。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BedError {
    /// 配置里有 [`TOKEN_PLACEHOLDER`] 但没拿到 token(未配置或钥匙串不可读)。
    MissingToken,
    /// 端点返回非 2xx;`detail` 是响应体片段(截断),通常含服务端报错。
    HttpStatus { code: u16, detail: String },
    /// 传输层错误(连接失败、读中断、非法请求头等)。
    Http(String),
    /// 响应 JSON 里按 `path` 取不到 URL(路径写错,或服务端返回了失败结构)。
    UrlPathMiss { path: String, body: String },
}

impl std::fmt::Display for BedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BedError::MissingToken => {
                write!(f, "图床需要 token:请在 设置 → 图片 里保存该图床的 token")
            }
            BedError::HttpStatus { code, detail } => write!(f, "图床返回 HTTP {code}:{detail}"),
            BedError::Http(msg) => f.write_str(msg),
            BedError::UrlPathMiss { path, body } => {
                write!(f, "图床响应里取不到 {path}:{body}")
            }
        }
    }
}

impl std::error::Error for BedError {}

/// 展开模板里的占位符:值里的 [`TOKEN_PLACEHOLDER`] 换 token、
/// [`NAME_PLACEHOLDER`] 换 percent-encode 后的文件名。
///
/// 出现 token 占位而 token 未提供(空或 `None`)→ [`BedError::MissingToken`]:
/// 静默发出无凭请求只会换来一个 401,不如在本地就把原因说清。
pub fn expand_placeholders(
    template: &str,
    token: Option<&str>,
    file_name: Option<&str>,
) -> Result<String, BedError> {
    let mut out = template.to_owned();
    if let Some(name) = file_name {
        out = out.replace(NAME_PLACEHOLDER, &encode_path_segment(name));
    }
    if out.contains(TOKEN_PLACEHOLDER) {
        let token = token.map(str::trim).filter(|token| !token.is_empty());
        match token {
            Some(token) => out = out.replace(TOKEN_PLACEHOLDER, token),
            None => return Err(BedError::MissingToken),
        }
    }
    Ok(out)
}

/// 文件名 → URL 路径段:非「非保留字符」一律 `%XX`。中文与空格必编(裸放
/// 会截断或被服务端拒),`/` 也编(文件名不该再含子路径)。
fn encode_path_segment(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for byte in name.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// 手写点分路径抽取(**不引 jsonpath**,C 段决策):`data.url`、
/// `content.download_url` 逐段下钻,终点必须是字符串。
///
/// 空段(`a..b`)与首尾空白宽容( trim );数组下标不支持 —— 三张预置模板
/// 与主流自建图床都不需要,需要时再加。
pub fn extract_url(value: &serde_json::Value, path: &str) -> Result<String, BedError> {
    let miss = || BedError::UrlPathMiss {
        path: path.to_owned(),
        body: truncate_chars(&value.to_string(), 200),
    };
    let mut node = value;
    for segment in path.split('.').map(str::trim).filter(|s| !s.is_empty()) {
        node = node.get(segment).ok_or_else(miss)?;
    }
    node.as_str().map(str::to_owned).ok_or_else(miss)
}

/// 返回值拼最终落文档的 URL:图床返回的已是完整 URL(含 `://`)时原样,
/// 是路径时拼 `url_prefix`(前缀为空也原样 —— 配置问题,插入的链接是否
/// 可用由浏览器/预览判定,不如实反映配置比猜更好)。
pub fn compose_url(extracted: &str, url_prefix: Option<&str>) -> String {
    if extracted.contains("://") {
        extracted.to_owned()
    } else {
        format!("{}{}", url_prefix.unwrap_or_default(), extracted)
    }
}

/// 一次上传的完整请求计划(纯数据,可穷尽单测):执行层只管照单发。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadPlan {
    /// HTTP 方法由 [`BedBody`] 决定。
    pub method: &'static str,
    /// 已展开占位符的端点 URL。
    pub url: String,
    /// 已展开占位符的请求头。
    pub headers: Vec<(String, String)>,
    /// 请求体。
    pub body: UploadBody,
}

/// [`UploadPlan`] 的体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadBody {
    /// multipart 表单:字段名 + 文件名 + MIME。
    Multipart {
        /// 表单字段名(来自 `file_field`)。
        field: String,
        /// 上报的文件名。
        file_name: String,
        /// 按扩展名推导的 MIME。
        mime: String,
    },
    /// base64 JSON(GitHub Contents API):已编码的完整请求体。
    Base64Json(String),
}

/// 推导一次上传的全部请求参数。占位符替换失败(token 缺失)在此报错,
/// 不会发出任何网络请求。
pub fn plan_upload(
    profile: &BedProfile,
    token: Option<&str>,
    bytes: &[u8],
    file_name: &str,
) -> Result<UploadPlan, BedError> {
    let url = expand_placeholders(&profile.api_url, token, Some(file_name))?;
    let mut headers = Vec::with_capacity(profile.headers.len());
    for (name, value) in &profile.headers {
        headers.push((name.clone(), expand_placeholders(value, token, None)?));
    }
    let body = match profile.body {
        BedBody::Multipart => UploadBody::Multipart {
            field: profile.file_field.clone(),
            file_name: file_name.to_owned(),
            mime: guess_mime(file_name).to_owned(),
        },
        BedBody::Base64Json => {
            use base64::Engine as _;
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            let body = serde_json::json!({
                "message": format!("Upload {file_name} via LaterMD"),
                "content": encoded,
            });
            UploadBody::Base64Json(body.to_string())
        }
    };
    let method = match profile.body {
        BedBody::Multipart => "POST",
        BedBody::Base64Json => "PUT",
    };
    Ok(UploadPlan {
        method,
        url,
        headers,
        body,
    })
}

/// 扩展名 → MIME(对话框白名单 PNG/JPEG/WebP/GIF + 兜底)。
fn guess_mime(file_name: &str) -> &'static str {
    match file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
    {
        Some(ext) if ext == "png" => "image/png",
        Some(ext) if ext == "jpg" || ext == "jpeg" => "image/jpeg",
        Some(ext) if ext == "gif" => "image/gif",
        Some(ext) if ext == "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

/// 按字符截断(错误文案里的响应体片段用;CJK 不劈开)。
pub(crate) fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        let mut cut: String = text.chars().take(max).collect();
        cut.push('…');
        cut
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 点分路径:嵌套对象逐段下钻取字符串;路径缺失 / 终点非字符串 /
    /// 空路径 → `UrlPathMiss`(带截断的响应体,便于定位配置错在哪)。
    #[test]
    fn dot_path_extracts_nested_strings() {
        let body = json!({ "data": { "url": "https://cdn/x.png" }, "code": "success" });
        assert_eq!(extract_url(&body, "data.url").unwrap(), "https://cdn/x.png");
        // 首尾空白与空段宽容
        assert_eq!(
            extract_url(&body, " data.url ").unwrap(),
            "https://cdn/x.png"
        );
        // GitHub:两层嵌套
        let github = json!({ "content": { "download_url": "https://raw/g/x.png" } });
        assert_eq!(
            extract_url(&github, "content.download_url").unwrap(),
            "https://raw/g/x.png"
        );

        for bad in ["data.miss", "data", "miss.url", ""] {
            let err = extract_url(&body, bad).unwrap_err();
            assert!(matches!(err, BedError::UrlPathMiss { .. }), "{bad}: {err}");
        }
        // 终点是数字不是字符串:同样取不到
        let counts = json!({ "data": { "width": 640 } });
        assert!(matches!(
            extract_url(&counts, "data.width"),
            Err(BedError::UrlPathMiss { .. })
        ));
    }

    /// `${TOKEN}` 替换:有 token 原样替换;没 token → MissingToken;不含
    /// 占位符的值不碰 token 也照常通过。
    #[test]
    fn token_placeholder_substitutes_or_fails() {
        assert_eq!(
            expand_placeholders("Bearer ${TOKEN}", Some("secret-1"), None).unwrap(),
            "Bearer secret-1"
        );
        // 同一串里出现多次
        assert_eq!(
            expand_placeholders("${TOKEN}&${TOKEN}", Some("k"), None).unwrap(),
            "k&k"
        );
        assert_eq!(
            expand_placeholders("Bearer ${TOKEN}", None, None).unwrap_err(),
            BedError::MissingToken
        );
        assert_eq!(
            expand_placeholders("Bearer ${TOKEN}", Some("  "), None).unwrap_err(),
            BedError::MissingToken,
            "空白 token 等价未配置"
        );
        // 不需要 token 的图床:无占位符直接放行
        assert_eq!(expand_placeholders("public", None, None).unwrap(), "public");
    }

    /// `${NAME}` 替换:percent-encode(中文、空格、`/`),非保留字符不动。
    #[test]
    fn name_placeholder_percent_encodes() {
        assert_eq!(
            expand_placeholders("https://api/x/${NAME}", None, Some("屏幕 截图.png")).unwrap(),
            "https://api/x/%E5%B1%8F%E5%B9%95%20%E6%88%AA%E5%9B%BE.png"
        );
        assert_eq!(
            expand_placeholders("u/${NAME}", None, Some("a_b-c.png")).unwrap(),
            "u/a_b-c.png",
            "非保留字符不编码"
        );
        assert_eq!(
            expand_placeholders("u/${NAME}", None, Some("sub/x.png")).unwrap(),
            "u/sub%2Fx.png",
            "文件名里的 / 编码,不引入子路径"
        );
        // 没有 NAME 占位符的端点(SM.MS)不吃文件名
        assert_eq!(
            expand_placeholders("https://sm.ms/api/v2/upload", None, Some("x.png")).unwrap(),
            "https://sm.ms/api/v2/upload"
        );
    }

    /// 表单字段拼装(SM.MS 口径):POST + Authorization 已替换 + 字段名
    /// `smfile` + MIME 按扩展名。
    #[test]
    fn plan_assembles_smms_multipart() {
        let mut profile = BedProfile::preset_smms();
        profile.id = "p1".to_owned();
        let plan = plan_upload(&profile, Some("tk"), b"bytes", "图.png").unwrap();
        assert_eq!(plan.method, "POST");
        assert_eq!(plan.url, "https://sm.ms/api/v2/upload");
        assert_eq!(
            plan.headers,
            vec![("Authorization".to_owned(), "tk".to_owned())]
        );
        assert_eq!(
            plan.body,
            UploadBody::Multipart {
                field: "smfile".to_owned(),
                file_name: "图.png".to_owned(),
                mime: "image/png".to_owned(),
            }
        );
        // token 缺失在计划层就拦下,不发出请求
        assert_eq!(
            plan_upload(&profile, None, b"bytes", "a.png").unwrap_err(),
            BedError::MissingToken
        );
    }

    /// 表单字段拼装(GitHub 口径):PUT + base64 JSON 体 + URL 里的文件名
    /// 已编码;Accept 头无占位符原样保留。
    #[test]
    fn plan_assembles_github_base64_json() {
        let mut profile = BedProfile::preset_github();
        profile.id = "p2".to_owned();
        let plan = plan_upload(&profile, Some("ghp_x"), b"abc", "截 图.png").unwrap();
        assert_eq!(plan.method, "PUT");
        assert!(
            plan.url
                .ends_with("/contents/images/%E6%88%AA%20%E5%9B%BE.png"),
            "{}",
            plan.url
        );
        assert_eq!(
            plan.headers,
            vec![
                ("Authorization".to_owned(), "Bearer ghp_x".to_owned()),
                (
                    "Accept".to_owned(),
                    "application/vnd.github+json".to_owned()
                ),
            ]
        );
        match &plan.body {
            UploadBody::Base64Json(json) => {
                let value: serde_json::Value = serde_json::from_str(json).unwrap();
                // base64 标准字母表编码 "abc"
                assert_eq!(value["content"], "YWJj");
                assert!(value["message"].as_str().unwrap().contains("截 图.png"));
            }
            other => panic!("应是 base64 JSON:{other:?}"),
        }
    }

    /// MIME 推导:白名单四格式 + 大写扩展名 + 兜底 octet-stream。
    #[test]
    fn mime_follows_extension() {
        for (name, mime) in [
            ("a.png", "image/png"),
            ("a.jpg", "image/jpeg"),
            ("a.jpeg", "image/jpeg"),
            ("a.webp", "image/webp"),
            ("a.gif", "image/gif"),
            ("A.PNG", "image/png"),
            ("a", "application/octet-stream"),
            ("a.bmp", "application/octet-stream"),
        ] {
            let plan = plan_upload(&BedProfile::preset_smms(), Some("t"), b"", name).unwrap();
            match plan.body {
                UploadBody::Multipart { mime: got, .. } => assert_eq!(got, mime, "{name}"),
                other => panic!("应是 multipart:{other:?}"),
            }
        }
    }

    /// `url_prefix`:完整 URL 原样;路径拼前缀;无前缀的路径也原样返回。
    #[test]
    fn url_prefix_only_prepends_paths() {
        assert_eq!(
            compose_url("https://cdn/x.png", Some("https://cdn/")),
            "https://cdn/x.png"
        );
        assert_eq!(
            compose_url("/i/2024/x.png", Some("https://cdn"),),
            "https://cdn/i/2024/x.png"
        );
        assert_eq!(compose_url("i/x.png", None), "i/x.png");
    }

    /// 序列化红线:beds.json 里 grep 不到 token(结构无 token 字段,headers
    /// 里只有占位符);预置模板往返无损。
    #[test]
    fn serialization_has_no_token_and_round_trips() {
        let mut smms = BedProfile::preset_smms();
        smms.id = "id-1".to_owned();
        smms.name = "我的 SM.MS".to_owned();
        let json = serde_json::to_string_pretty(&smms).unwrap();
        assert!(!json.contains("token"), "不出现 token 字段:{json}");
        // 占位符形态可以出现(它不是秘密)
        assert!(json.contains("${TOKEN}"));
        // 未知 token 真值不可能出现:字段穷举即可证明
        let back: BedProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back, smms);

        // 三张模板都能序列化往返(body 的 snake_case 名一并钉住)
        for mut preset in [
            BedProfile::preset_smms(),
            BedProfile::preset_github(),
            BedProfile::preset_custom(),
        ] {
            preset.id = "x".to_owned();
            let json = serde_json::to_string(&preset).unwrap();
            assert!(json.contains("\"body\""));
            let back: BedProfile = serde_json::from_str(&json).unwrap();
            assert_eq!(back, preset);
        }
    }

    /// 归一化:trim、空前缀折 None、空名字的请求头丢弃。
    #[test]
    fn normalize_trims_and_drops_empty() {
        let mut profile = BedProfile {
            id: "i".to_owned(),
            name: "  名字  ".to_owned(),
            api_url: " https://x ".to_owned(),
            file_field: " file ".to_owned(),
            headers: vec![
                ("  ".to_owned(), "v".to_owned()),
                ("Authorization".to_owned(), " Bearer k ".to_owned()),
            ],
            url_path: " data.url ".to_owned(),
            url_prefix: Some("   ".to_owned()),
            body: BedBody::Multipart,
        };
        profile.normalize();
        assert_eq!(profile.name, "名字");
        assert_eq!(profile.api_url, "https://x");
        assert_eq!(profile.file_field, "file");
        assert_eq!(profile.url_path, "data.url");
        assert_eq!(profile.url_prefix, None, "空白前缀折成 None");
        assert_eq!(
            profile.headers,
            vec![("Authorization".to_owned(), "Bearer k".to_owned())]
        );
    }

    /// 未命名 profile 的显示名退路。
    #[test]
    fn display_name_falls_back() {
        assert_eq!(BedProfile::preset_smms().display_name(), "SM.MS");
        assert_eq!(BedProfile::preset_custom().display_name(), "(未命名)");
    }

    /// 错误文案:四类变体都可读、可上提示行,且不含 token。
    #[test]
    fn error_messages_are_user_facing() {
        assert!(BedError::MissingToken.to_string().contains("token"));
        assert_eq!(
            BedError::HttpStatus {
                code: 401,
                detail: "unauthorized".to_owned()
            }
            .to_string(),
            "图床返回 HTTP 401:unauthorized"
        );
        assert_eq!(
            BedError::Http("连接失败".to_owned()).to_string(),
            "连接失败"
        );
        let miss = BedError::UrlPathMiss {
            path: "data.url".to_owned(),
            body: "{\"code\":\"error\"}".to_owned(),
        };
        assert!(miss.to_string().contains("data.url"));
    }
}
