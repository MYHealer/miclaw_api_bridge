use super::{
    build_http_client, strip_prefix, AuthState, LoginFlowContext, Session, SERVICE_LOGIN_AUTH2_URL,
    SERVICE_LOGIN_URL, SID,
};
use crate::auth::two_factor::extract_query_param;
use crate::error::{BridgeError, Result};
use crate::storage::Storage;
use md5::{Digest, Md5};
use parking_lot::RwLock;
use reqwest::Client;
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LoginRequest {
    pub account: String,
    pub password: String,
    #[serde(default)]
    pub captcha: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum LoginOutcome {
    /// Login succeeded; session has been written.
    Authenticated { nick: Option<String> },
    /// 2FA required, the front-end should pick a method (4=phone, 8=email)
    /// from `options` then ask for a verification ticket.
    TwoFactorRequired { options: Vec<i32> },
    /// Server requires a captcha. The URL points to an image returned by
    /// `account.xiaomi.com/pass/getCode`.
    CaptchaRequired { captcha_url: String },
    Failed { code: i64, description: String },
}

pub async fn login(
    state: &Arc<RwLock<AuthState>>,
    storage: &Arc<Storage>,
    req: LoginRequest,
) -> Result<LoginOutcome> {
    if req.account.is_empty() || req.password.is_empty() {
        return Err(BridgeError::Login("empty credentials".into()));
    }

    // Each login attempt starts from a fresh cookie jar so leftovers from a
    // previous failed attempt do not contaminate the new one.
    let (client, _) = build_http_client(&Session::default())?;

    // Step 1: GET serviceLogin to seed _sign / qs / callback.
    let pre = client
        .get(SERVICE_LOGIN_URL)
        .send()
        .await
        .map_err(BridgeError::from)?
        .text()
        .await?;
    let pre_json: Value = serde_json::from_str(strip_prefix(&pre))
        .map_err(|e| BridgeError::Login(format!("pre-login parse: {e}")))?;
    let sign = pre_json
        .get("_sign")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let qs = pre_json
        .get("qs")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let callback = pre_json
        .get("callback")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Step 2: POST serviceLoginAuth2.
    let hash = md5_upper(&req.password);
    let mut form = vec![
        ("user", req.account.clone()),
        ("hash", hash),
        ("sid", SID.to_string()),
        ("_json", "true".into()),
        ("_sign", sign),
        ("qs", qs),
        ("callback", callback),
    ];
    if let Some(c) = req.captcha.as_ref() {
        if !c.is_empty() {
            form.push(("captCode", c.clone()));
        }
    }
    let resp = client
        .post(SERVICE_LOGIN_AUTH2_URL)
        .form(&form)
        .send()
        .await?
        .text()
        .await?;
    let body: Value = serde_json::from_str(strip_prefix(&resp))
        .map_err(|e| BridgeError::Login(format!("auth2 parse: {e}")))?;

    let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    let description = body
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Captcha flow: server signals 87001 with a captchaUrl pointing to a JPG.
    if let Some(captcha_url) = body.get("captchaUrl").and_then(|v| v.as_str()) {
        let mut guard = state.write();
        guard.flow = LoginFlowContext {
            captcha_url: Some(captcha_url.to_string()),
            ..Default::default()
        };
        return Ok(LoginOutcome::CaptchaRequired {
            captcha_url: captcha_url.to_string(),
        });
    }

    let notification_url = body.get("notificationUrl").and_then(|v| v.as_str());
    if let Some(url) = notification_url {
        if !url.is_empty() && url != "null" {
            // 2FA: fetch identity/list -> available options.
            let list_url = url.replace("fe/service/identityauthStart", "identity/list");
            let list_resp = client.get(&list_url).send().await?;
            let id_session = list_resp
                .cookies()
                .find(|c| c.name() == "identity_session")
                .map(|c| c.value().to_string());
            let body = list_resp.text().await?;
            let list_json: Value = serde_json::from_str(strip_prefix(&body))
                .map_err(|e| BridgeError::Login(format!("identity/list parse: {e}")))?;
            let options: Vec<i32> = list_json
                .get("options")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_i64().map(|n| n as i32))
                        .collect()
                })
                .unwrap_or_default();

            let context = extract_query_param(url, "context").map(|s| s.to_string());
            let mut guard = state.write();
            guard.flow = LoginFlowContext {
                identity_session: id_session,
                notification_url: Some(url.to_string()),
                two_factor_options: options.clone(),
                captcha_url: None,
            };
            // Save context inside session as a hint; not strictly required.
            let _ = context;
            return Ok(LoginOutcome::TwoFactorRequired { options });
        }
    }

    // Single-step login: pull out final fields.
    if code != 0 {
        return Ok(LoginOutcome::Failed { code, description });
    }
    let session = parse_session_fields(&body);
    let location = body
        .get("location")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BridgeError::Login("missing location".into()))?
        .to_string();

    let session = finalize_with_location(&client, location, session).await?;

    {
        let mut guard = state.write();
        guard.session = session.clone();
        guard.flow = LoginFlowContext::default();
        guard.save(storage)?;
    }

    Ok(LoginOutcome::Authenticated { nick: session.nick })
}

pub fn md5_upper(input: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    let bytes = hasher.finalize();
    hex::encode_upper(bytes)
}

pub(crate) fn parse_session_fields(body: &Value) -> Session {
    let pluck = |k: &str| body.get(k).and_then(|v| v.as_str()).map(|s| s.to_string());
    let mut nick = pluck("nick");
    if nick.as_deref().map(str::is_empty).unwrap_or(true) {
        nick = pluck("nickName");
    }
    Session {
        user_id: pluck("userId"),
        c_user_id: pluck("cUserId"),
        pass_token: pluck("passToken"),
        ssecurity: pluck("ssecurity"),
        nick,
        ..Default::default()
    }
}

/// Following `location` returns a 200 with `Set-Cookie: serviceToken=...`.
pub(crate) async fn finalize_with_location(
    client: &Client,
    location: String,
    mut session: Session,
) -> Result<Session> {
    let url = if location.contains("_userIdNeedEncrypt") {
        location
    } else {
        // miclaw appends this flag to make sure cUserId is encrypted in cookies.
        if location.contains('?') {
            format!("{location}&_userIdNeedEncrypt=true")
        } else {
            format!("{location}?_userIdNeedEncrypt=true")
        }
    };
    let resp = client.get(&url).send().await?;
    if let Some(token) = resp
        .cookies()
        .find(|c| c.name() == "serviceToken")
        .map(|c| c.value().to_string())
    {
        session.service_token = Some(token);
    }
    if session.service_token.is_none() {
        return Err(BridgeError::Login(
            "serviceToken missing after redirect".into(),
        ));
    }
    session.refreshed_at = Some(chrono::Utc::now().timestamp_millis());
    Ok(session)
}
