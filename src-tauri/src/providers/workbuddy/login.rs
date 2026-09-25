use std::{collections::HashMap, sync::Mutex, time::Duration};

use chrono::Utc;
use rand::{rng, RngCore};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use thiserror::Error;

use super::{client::USER_AGENT, session::WorkBuddySession};
use crate::models::DeviceCodeChallenge;

/// 一次待授权尝试的有效期（秒）。用户要去浏览器里完成确认，所以给足时间；
/// TTL 同时决定一个被遗忘的 state 何时从内存中消失。
pub const LOGIN_TTL_SECONDS: u64 = 600;

const LOGIN_TTL_MS: i64 = LOGIN_TTL_SECONDS as i64 * 1000;
const PLATFORM: &str = "workbuddy";
const STATE_PATH: &str = "/v2/plugin/auth/state";
const TOKEN_PATH: &str = "/v2/plugin/auth/token";
const ACCOUNT_PATH: &str = "/v2/plugin/login/account";

/// 只有这几个域名下的凭据才会被后续请求使用：域名决定请求发往哪台主机，
/// 也决定 `X-Domain` 头，因此白名单之外的域名一律丢弃、绝不落库。
const ALLOWED_DOMAINS: &[&str] = &[
    "www.codebuddy.cn",
    "codebuddy.cn",
    "www.workbuddy.cn",
    "workbuddy.cn",
];

const HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WorkBuddyLoginError {
    #[error("Could not reach WorkBuddy.")]
    Connection,
    #[error("WorkBuddy returned an invalid response.")]
    InvalidResponse,
}

/// 轮询结果只在 Rust 内部流转：`Ready` 带上会话，
/// 由 Task 5 存入 vault 之后再换成不含凭据的 `DeviceCodePoll` 交给前端。
#[derive(Debug)]
pub enum LoginPoll {
    Pending,
    Ready(Box<WorkBuddySession>),
    Failed(String),
}

/// 一次进行中的设备码尝试。`state` 只存在于服务端与 Rust 之间，
/// `login_id` 才是交给前端的句柄，两者刻意分开。
struct PendingLogin {
    state: String,
    expires_at_ms: i64,
    done: bool,
}

pub struct DeviceCodeLogin {
    client: Client,
    base_url: String,
    pending: Mutex<HashMap<String, PendingLogin>>,
}

impl DeviceCodeLogin {
    pub fn new(base_url: &str) -> Result<Self, WorkBuddyLoginError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(30))
            // 绝不跟随重定向：取资料的请求带着刚签发的 bearer token，换 token 的请求带着
            // 服务端 state，而 3xx 的 Location 指向哪台主机由响应决定。跟随它等于把凭据交给
            // 域名白名单从未批准的目的地；不跟随则 3xx 只是一个普通的不成功响应，
            // poll 保持 Pending、start 报失败。
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(USER_AGENT)
            .build()
            .map_err(|_| WorkBuddyLoginError::Connection)?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_owned(),
            pending: Mutex::new(HashMap::new()),
        })
    }

    /// 申请一个授权 state，并把它藏进内部表里。
    /// 返回给调用方的只有一个与 state 无关的登录 id。
    pub fn start(&self) -> Result<DeviceCodeChallenge, WorkBuddyLoginError> {
        let url = format!("{}{STATE_PATH}?platform={PLATFORM}", self.base_url);
        let started = std::time::Instant::now();
        let response = self
            .client
            .post(&url)
            .json(&json!({}))
            .header("Accept", "application/json, text/plain, */*")
            .send()
            .map_err(|_| {
                crate::app_warn!("http", "workbuddy login state request failed (transport)");
                WorkBuddyLoginError::Connection
            })?;
        let status = response.status();
        crate::app_debug!(
            "http",
            "workbuddy login state HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        let body = response
            .json::<Value>()
            .map_err(|_| WorkBuddyLoginError::InvalidResponse)?;
        if !status.is_success() {
            return Err(WorkBuddyLoginError::InvalidResponse);
        }
        let data = body.get("data").unwrap_or(&body);
        let state =
            non_empty_string(data, &["state"]).ok_or(WorkBuddyLoginError::InvalidResponse)?;
        // 授权地址只来自响应体：一旦带着 `file:` 之类的 scheme 进入挑战，界面显示它、
        // 系统默认处理器打开它，都会把本地启动能力交给一个被劫持的响应。所以在解析边界上
        // 就把非 https 的地址判为无效响应，绝不让它变成一个挑战。
        let verification_uri = non_empty_string(data, &["authUrl", "auth_url", "url"])
            .filter(|uri| uri.starts_with("https://"))
            .ok_or(WorkBuddyLoginError::InvalidResponse)?;
        let login_id = self.register(
            &state,
            Utc::now().timestamp_millis().saturating_add(LOGIN_TTL_MS),
        );
        Ok(DeviceCodeChallenge {
            login_id,
            verification_uri,
            expires_in: LOGIN_TTL_SECONDS,
        })
    }

    /// 查一次登录是否已经完成。
    ///
    /// 网络抖动、非 JSON 响应、服务端还没授权都只是「这次没结果」而不是失败：
    /// 它们都由 TTL 兜底，不能因为一次抖动就终结用户正在浏览器里做的授权。
    pub fn poll(&self, login_id: &str) -> LoginPoll {
        let state = match self.active_state(login_id) {
            Ok(state) => state,
            Err(message) => return LoginPoll::Failed(message),
        };

        let started = std::time::Instant::now();
        let response = self
            .client
            .get(format!("{}{TOKEN_PATH}", self.base_url))
            .query(&[("state", state.as_str())])
            .header("Accept", "application/json, text/plain, */*")
            .send();
        let response = match response {
            Ok(response) => response,
            Err(_) => {
                crate::app_warn!("http", "workbuddy login token request failed (transport)");
                return LoginPoll::Pending;
            }
        };
        let status = response.status();
        crate::app_debug!(
            "http",
            "workbuddy login token HTTP {} ({}ms)",
            status.as_u16(),
            started.elapsed().as_millis()
        );
        let body = match response.json::<Value>() {
            Ok(body) => body,
            Err(_) => return LoginPoll::Pending,
        };
        if !status.is_success() {
            return LoginPoll::Pending;
        }
        let data = body.get("data").unwrap_or(&body);
        let code = body
            .get("code")
            .or_else(|| data.get("code"))
            .and_then(number_i64);
        // 只有服务端明确报成功（code 0/200）才算授权完成：
        // 缺 code 的响应证明不了授权已生效，继续等下一次轮询。
        if !matches!(code, Some(0) | Some(200)) {
            return LoginPoll::Pending;
        }
        let Some(access_token) = non_empty_string(data, &["accessToken", "access_token"]) else {
            return LoginPoll::Pending;
        };
        // 域名决定之后把凭据和请求发往哪台主机，所以它必须来自服务端且在白名单内；
        // 缺失域名同样不可信：宁可让用户重新登录，也不替服务端猜一个域名。
        let domain = match non_empty_string(data, &["domain"]) {
            Some(domain) if ALLOWED_DOMAINS.contains(&domain.as_str()) => domain,
            Some(domain) => {
                self.mark_done(login_id);
                return LoginPoll::Failed(format!(
                    "WorkBuddy returned credentials for an untrusted domain ({domain}). Sign in again."
                ));
            }
            None => {
                self.mark_done(login_id);
                return LoginPoll::Failed(
                    "WorkBuddy returned credentials without a domain. Sign in again.".to_owned(),
                );
            }
        };

        let now_ms = Utc::now().timestamp_millis();
        let mut session = WorkBuddySession {
            access_token,
            refresh_token: non_empty_string(data, &["refreshToken", "refresh_token"]),
            token_type: non_empty_string(data, &["tokenType", "token_type"])
                .unwrap_or_else(|| "Bearer".to_owned()),
            domain,
            uid: None,
            nickname: None,
            email: None,
            enterprise_id: None,
            expires_at: expiry(
                data,
                &["expiresAt", "expires_at"],
                &["expiresIn", "expires_in"],
                now_ms,
            ),
            refresh_expires_at: expiry(
                data,
                &["refreshExpiresAt", "refresh_expires_at"],
                &["refreshExpiresIn", "refresh_expires_in"],
                now_ms,
            ),
        };
        // 资料请求放在取消判定之前：`mark_done` 之后的语义（取消优先）保持与改动前完全一致，
        // 资料只是给这份会话补充展示信息。
        self.enrich_profile(&state, &mut session);
        // 请求期间用户可能刚点了取消：取消优先，不能把凭据交给一个已作废的尝试。
        if !self.mark_done(login_id) {
            return LoginPoll::Failed(
                "This WorkBuddy sign-in attempt is no longer active. Start again.".to_owned(),
            );
        }
        LoginPoll::Ready(Box::new(session))
    }

    /// 取账号资料并合并进会话。
    ///
    /// 资料只是展示信息：传输失败、非成功状态、不可解析的响应都保持 token-only 的会话，
    /// 一份已经能用的凭据绝不因为资料拿不到而被丢弃。响应体不打印，避免把用户资料写进日志。
    fn enrich_profile(&self, state: &str, session: &mut WorkBuddySession) {
        let response = self
            .client
            .get(format!("{}{ACCOUNT_PATH}", self.base_url))
            .query(&[("state", state)])
            .header("Authorization", format!("Bearer {}", session.access_token))
            .header("X-Domain", session.domain.as_str())
            .header("Accept", "application/json, text/plain, */*")
            .send();
        let Ok(response) = response else {
            crate::app_warn!("http", "workbuddy login account request failed (transport)");
            return;
        };
        if !response.status().is_success() {
            return;
        }
        let Ok(body) = response.json::<Value>() else {
            return;
        };
        let data = body.get("data").unwrap_or(&body);
        if let Some(uid) = non_empty_string(data, &["uid"]) {
            session.uid = Some(uid);
        }
        if let Some(nickname) = non_empty_string(data, &["nickname", "nick_name"]) {
            session.nickname = Some(nickname);
        }
        if let Some(email) = non_empty_string(data, &["email"]) {
            session.email = Some(email);
        }
        if let Some(enterprise_id) = non_empty_string(data, &["enterpriseId", "enterprise_id"]) {
            session.enterprise_id = Some(enterprise_id);
        }
    }

    /// 取消一次尝试。返回它是否真的从「进行中」变成了「已结束」：
    /// 未知的 id 与已经结束的尝试都返回 false，重复取消不会让调用方误以为又取消了一次。
    pub fn cancel(&self, login_id: &str) -> bool {
        self.mark_done(login_id)
    }

    /// 登记一次尝试，并顺手清掉已结束或已过期的旧尝试，避免这张表一直变大。
    fn register(&self, state: &str, expires_at_ms: i64) -> String {
        let login_id = generate_login_id();
        let now_ms = Utc::now().timestamp_millis();
        let mut pending = self.pending.lock().unwrap();
        pending.retain(|_, entry| !entry.done && entry.expires_at_ms > now_ms);
        pending.insert(
            login_id.clone(),
            PendingLogin {
                state: state.to_owned(),
                expires_at_ms,
                done: false,
            },
        );
        login_id
    }

    /// 取出仍在进行中的尝试对应的服务端 state。
    /// 未知、已取消、已过期都以用户看得懂的文案失败，并终结这次尝试。
    fn active_state(&self, login_id: &str) -> Result<String, String> {
        let mut pending = self.pending.lock().unwrap();
        let Some(entry) = pending.get_mut(login_id) else {
            return Err("This WorkBuddy sign-in attempt is unknown. Start again.".to_owned());
        };
        if entry.done {
            return Err(
                "This WorkBuddy sign-in attempt is no longer active. Start again.".to_owned(),
            );
        }
        if entry.expires_at_ms <= Utc::now().timestamp_millis() {
            entry.done = true;
            return Err("This WorkBuddy sign-in attempt expired. Start again.".to_owned());
        }
        Ok(entry.state.clone())
    }

    /// 终结一次尝试，返回它此前是否仍在进行中。
    /// 取消、成功、失败都走这里，保证一次尝试只会从「进行中」终结一次。
    fn mark_done(&self, login_id: &str) -> bool {
        match self.pending.lock().unwrap().get_mut(login_id) {
            Some(entry) if !entry.done => {
                entry.done = true;
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
impl DeviceCodeLogin {
    pub(crate) fn for_test(base_url: &str) -> Self {
        Self::new(base_url).expect("a test login client should build")
    }

    /// 直接登记一个待授权尝试。`test_http::serve_once` 只接受一次连接，
    /// 先 `start()` 再 `poll()` 会让轮询打到已关闭的服务器上，
    /// 从而因为错误的原因通过，所以 `poll`/`cancel` 的测试从这里进入。
    pub(crate) fn register_for_test(&self, state: &str) -> String {
        self.register(
            state,
            Utc::now().timestamp_millis().saturating_add(LOGIN_TTL_MS),
        )
    }

    /// 登记一个已经过期的尝试，用来钉住 TTL 这条边界。
    pub(crate) fn register_expired_for_test(&self, state: &str) -> String {
        self.register(state, Utc::now().timestamp_millis() - 1)
    }
}

/// 前端拿到的句柄用随机值而不是可猜的序号，
/// 免得本机另一个进程凭 id 去轮询别人正在进行的登录。
fn generate_login_id() -> String {
    let mut bytes = [0_u8; 16];
    rng().fill_bytes(&mut bytes);
    let mut id = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        id.push(HEX[(byte >> 4) as usize] as char);
        id.push(HEX[(byte & 0x0f) as usize] as char);
    }
    id
}

/// 兼容 `authUrl`/`auth_url` 这类同一个字段的不同写法，跳过空字符串。
fn non_empty_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    })
}

/// 兼容服务端把数字写成字符串的习惯（与 `client.rs` 的解析一致）。
fn number_i64(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_str()?.parse().ok())
}

/// `expiresAt` 是毫秒时间戳；服务端只给相对秒数时按当前时间换算，
/// 免得一份刚拿到的凭据被当成已过期丢掉。
fn expiry(data: &Value, absolute: &[&str], relative: &[&str], now_ms: i64) -> Option<i64> {
    if let Some(value) = absolute
        .iter()
        .find_map(|key| data.get(*key).and_then(number_i64))
    {
        return Some(value);
    }
    relative
        .iter()
        .find_map(|key| data.get(*key).and_then(number_i64))
        .filter(|seconds| *seconds > 0)
        .map(|seconds| now_ms.saturating_add(seconds.saturating_mul(1000)))
}
