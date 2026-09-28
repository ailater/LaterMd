//! 上传契约与 ureq 实现(C 段唯一的网络层)。
//!
//! [`BedUploader`] 是小 trait:预置的 [`UreqUploader`] 之外,app 侧测试注入
//! 假实现即可覆盖「发起 → 收流 → 插入」全链路,不需要真实网络。执行的就是
//! [`crate::plan_upload`] 产出的 [`UploadPlan`](计划层穷尽单测,这层保持薄)。

use std::time::Duration;

use ureq::unversioned::multipart::{Form, Part};

use crate::{
    compose_url, extract_url, plan_upload, truncate_chars, BedError, BedProfile, UploadBody,
};

/// 图床上传契约。阻塞式(调用方负责放后台线程,app 侧照抄 AI 流式的
/// spawn 手法);返回值是可直接落 Markdown 的 URL。
pub trait BedUploader: Send + Sync {
    /// 上传 `bytes`(文件名 `file_name`)到 `profile`,token 由调用方从
    /// 系统凭据读出传入 —— 本 crate 不碰钥匙串。
    fn upload(
        &self,
        profile: &BedProfile,
        token: Option<&str>,
        bytes: &[u8],
        file_name: &str,
    ) -> Result<String, BedError>;
}

/// ureq 实现:阻塞 HTTP,超时口径对齐 latermd-ai(连接 30s、响应 60s,
/// body 预算放宽到 300s —— 图床回包可能带大图元数据)。
#[derive(Debug, Clone)]
pub struct UreqUploader {
    agent: ureq::Agent,
}

impl Default for UreqUploader {
    fn default() -> Self {
        Self::new()
    }
}

impl UreqUploader {
    /// 默认超时的上传器。
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            .timeout_recv_body(Some(Duration::from_secs(300)))
            // 状态码手判:非 2xx 的响应体里有服务端报错,要走统一截断路径
            .http_status_as_error(false)
            .build()
            .into();
        Self { agent }
    }
}

impl BedUploader for UreqUploader {
    fn upload(
        &self,
        profile: &BedProfile,
        token: Option<&str>,
        bytes: &[u8],
        file_name: &str,
    ) -> Result<String, BedError> {
        self.execute(profile, token, bytes, file_name)
            .map_err(|error| redact(error, token))
    }
}

impl UreqUploader {
    fn execute(
        &self,
        profile: &BedProfile,
        token: Option<&str>,
        bytes: &[u8],
        file_name: &str,
    ) -> Result<String, BedError> {
        let plan = plan_upload(profile, token, bytes, file_name)?;
        let mut request = if plan.method == "PUT" {
            self.agent.put(&plan.url)
        } else {
            self.agent.post(&plan.url)
        };
        for (name, value) in &plan.headers {
            request = request.header(name, value);
        }
        let response = match &plan.body {
            UploadBody::Multipart {
                field,
                file_name,
                mime,
            } => {
                let part = Part::bytes(bytes)
                    .file_name(file_name)
                    .mime_str(mime)
                    .map_err(|error| BedError::Http(error.to_string()))?;
                let form = Form::new().part(field, part);
                request.send(form)
            }
            UploadBody::Base64Json(body) => {
                request.content_type("application/json").send(body.as_str())
            }
        }
        .map_err(|error| BedError::Http(error.to_string()))?;

        let status = response.status().as_u16();
        let body = response
            .into_body()
            .read_to_string()
            .map_err(|error| BedError::Http(error.to_string()))?;
        if !(200..300).contains(&status) {
            return Err(BedError::HttpStatus {
                code: status,
                detail: truncate_chars(&body, 400),
            });
        }
        let json: serde_json::Value =
            serde_json::from_str(&body).map_err(|_| BedError::UrlPathMiss {
                path: profile.url_path.clone(),
                body: truncate_chars(&body, 200),
            })?;
        let url = extract_url(&json, &profile.url_path)?;
        Ok(compose_url(&url, profile.url_prefix.as_deref()))
    }
}

/// 错误文案的最后一道消毒:token 可能出现在 URL 查询参数里,传输层错误
/// 又可能原样带出 URL —— 把 token 真值替换成掩码(与 latermd-creds 的
/// redact 同哲学)。
fn redact(error: BedError, token: Option<&str>) -> BedError {
    let Some(token) = token.filter(|token| !token.is_empty()) else {
        return error;
    };
    let scrub = |text: String| text.replace(token, "***");
    match error {
        BedError::HttpStatus { code, detail } => BedError::HttpStatus {
            code,
            detail: scrub(detail),
        },
        BedError::Http(message) => BedError::Http(scrub(message)),
        BedError::UrlPathMiss { path, body } => BedError::UrlPathMiss {
            path,
            body: scrub(body),
        },
        BedError::MissingToken => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 消毒闸:传输层错误文案里嵌了 token(经 URL 查询参数)也会被抹掉。
    /// 用反射手段从 BedError::Http 构造一个含 token 的文案验证替换。
    #[test]
    fn token_never_leaks_into_error_text() {
        let token = "placeholder-secret-token";
        let raw = BedError::Http(format!("https://x/api?token={token} 连接失败"));
        let redacted = redact(raw, Some(token));
        assert_eq!(
            redacted,
            BedError::Http("https://x/api?token=*** 连接失败".to_owned())
        );
        // 无 token(不需要凭据的图床)原样通过
        let raw = BedError::Http("连接失败".to_owned());
        assert_eq!(redact(raw, None), BedError::Http("连接失败".to_owned()));
        // HttpStatus / UrlPathMiss 的 detail 与 body 同一道闸
        let raw = BedError::HttpStatus {
            code: 401,
            detail: format!("bad token {token}"),
        };
        assert_eq!(
            redact(raw, Some(token)),
            BedError::HttpStatus {
                code: 401,
                detail: "bad token ***".to_owned()
            }
        );
    }

    /// 计划层错误(token 缺失)不经过网络直接返回;文案同样无 token。
    #[test]
    fn missing_token_short_circuits_before_network() {
        let uploader = UreqUploader::new();
        let mut profile = BedProfile::preset_smms();
        // 指向一个必然无法连接的地址:若真的发请求,会得到 Http 错而不是
        // MissingToken —— 断言靠错误类型区分「没发」与「发失败」
        profile.api_url = "https://127.0.0.1:1/upload".to_owned();
        assert_eq!(
            uploader.upload(&profile, None, b"x", "a.png").unwrap_err(),
            BedError::MissingToken
        );
    }
}
