//! Blocking HTTP client for the ledger CLI.
//!
//! Every CLI command that needs to talk to the server goes through this
//! module. It:
//!
//! * resolves the API base URL (from `LEDGER_API_URL` or the saved creds),
//! * attaches `Authorization: Bearer <access_token>` when a session exists,
//! * parses the standard response envelope and surfaces `errors` as a
//!   typed [`CliError`],
//! * transparently refreshes the access token on a single `401` and retries.
//!
//! The client is deliberately synchronous — CLI commands outside `start`
//! don't need a Tokio runtime, and blocking keeps the control flow obvious.

use std::error::Error;
use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::blocking::{Client as HttpClient, RequestBuilder, Response};
use reqwest::{Method, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

use super::config::{api_url, Credentials};

/// Error type for every CLI-side HTTP interaction.
#[derive(Debug)]
pub enum CliError {
    /// A transport-level failure (DNS, TLS, socket, etc).
    Transport(reqwest::Error),
    /// The server answered with an `errors` envelope.
    Api {
        status: u16,
        errors: Vec<ApiErrorObject>,
    },
    /// Server responded but neither `data` nor `errors` were present.
    EmptyResponse(u16),
    /// JSON could not be parsed.
    Decode(String),
    /// Local filesystem / IO error (credentials, workdir, etc.).
    Io(std::io::Error),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Transport(e) => write!(f, "network error: {e}"),
            CliError::Api { status, errors } => {
                if let Some(first) = errors.first() {
                    write!(
                        f,
                        "api {status} {}: {}",
                        first.code,
                        first.detail.as_deref().unwrap_or(&first.title)
                    )
                } else {
                    write!(f, "api {status}: (no error body)")
                }
            }
            CliError::EmptyResponse(status) => {
                write!(f, "server {status} returned an empty envelope")
            }
            CliError::Decode(e) => write!(f, "could not decode server response: {e}"),
            CliError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl Error for CliError {}

impl From<reqwest::Error> for CliError {
    fn from(e: reqwest::Error) -> Self {
        CliError::Transport(e)
    }
}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        CliError::Io(e)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(e: serde_json::Error) -> Self {
        CliError::Decode(e.to_string())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ApiErrorObject {
    pub status: u16,
    pub code: String,
    pub title: String,
    pub detail: Option<String>,
    #[serde(default)]
    pub source: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    #[serde(default = "Option::default")]
    data: Option<T>,
    #[serde(default = "Option::default")]
    errors: Option<Vec<ApiErrorObject>>,
}

#[derive(Debug, Deserialize)]
struct TokenPair {
    access_token: String,
    refresh_token: String,
    #[allow(dead_code)]
    token_type: String,
    access_expires_in: i64,
    refresh_expires_in: i64,
    #[serde(default)]
    stay_logged_in: bool,
}

/// Synchronous API client. Construct with [`Client::anonymous`] for
/// endpoints that don't need a login, or [`Client::authed`] to use the
/// stored credentials.
pub struct Client {
    http: HttpClient,
    base: String,
    creds: Option<Credentials>,
}

impl Client {
    fn http() -> Result<HttpClient, CliError> {
        HttpClient::builder()
            .user_agent(concat!("ledger-cli/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(CliError::from)
    }

    /// A client that doesn't attach any `Authorization` header. Use for
    /// `register`, `login`, and health probes.
    pub fn anonymous() -> Result<Self, CliError> {
        Ok(Self {
            http: Self::http()?,
            base: api_url(),
            creds: None,
        })
    }

    /// A client that loads and attaches the stored credentials. Fails if
    /// the user hasn't logged in.
    pub fn authed() -> Result<Self, CliError> {
        let creds = Credentials::require()?;
        Ok(Self {
            http: Self::http()?,
            base: creds.api_url.clone(),
            creds: Some(creds),
        })
    }

    /// Borrow the currently-loaded credentials, if any.
    pub fn credentials(&self) -> Option<&Credentials> {
        self.creds.as_ref()
    }

    /// Absolute URL for an endpoint path like `/v1/repos/{id}`.
    fn url(&self, path: &str) -> String {
        if path.starts_with('/') {
            format!("{}{}", self.base, path)
        } else {
            format!("{}/{}", self.base, path)
        }
    }

    fn add_auth(&self, req: RequestBuilder) -> RequestBuilder {
        match &self.creds {
            Some(c) if !c.access_token.is_empty() => req.bearer_auth(&c.access_token),
            _ => req,
        }
    }

    /// Performs a request and parses the envelope. Transparently retries
    /// once on a `401` if we have a refresh token available.
    fn execute<T: DeserializeOwned>(
        &mut self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<T, CliError> {
        let send = |this: &Self| -> Result<Response, CliError> {
            let mut req = this.http.request(method.clone(), this.url(path));
            if let Some(b) = body {
                req = req.json(b);
            }
            let req = this.add_auth(req);
            Ok(req.send()?)
        };

        let mut resp = send(self)?;
        if resp.status() == StatusCode::UNAUTHORIZED && self.creds.is_some() {
            if self.try_refresh()? {
                resp = send(self)?;
            }
        }
        parse_envelope(resp)
    }

    /// Attempts to exchange the refresh token for a fresh pair. Returns
    /// `Ok(true)` on success, `Ok(false)` if we had no refresh token or
    /// the server refused it (in which case the local creds are cleared
    /// so the next command prompts the user to log in again).
    fn try_refresh(&mut self) -> Result<bool, CliError> {
        let Some(creds) = self.creds.clone() else {
            return Ok(false);
        };
        if creds.refresh_token.is_empty() {
            return Ok(false);
        }

        let body = serde_json::json!({ "refresh_token": creds.refresh_token });
        let resp = self
            .http
            .post(self.url("/v1/auth/refresh"))
            .json(&body)
            .send()?;

        if !resp.status().is_success() {
            // Refresh denied → drop stored creds. User re-auths next time.
            let _ = Credentials::clear();
            self.creds = None;
            return Ok(false);
        }

        let pair: TokenPair = parse_envelope(resp)?;
        let refreshed = Credentials {
            access_token: pair.access_token,
            refresh_token: pair.refresh_token,
            stay_logged_in: pair.stay_logged_in,
            access_expires_at: expiry_from_ttl(pair.access_expires_in),
            refresh_expires_at: expiry_from_ttl(pair.refresh_expires_in),
            ..creds
        };
        refreshed.save()?;
        self.creds = Some(refreshed);
        Ok(true)
    }

    pub fn get<T: DeserializeOwned>(&mut self, path: &str) -> Result<T, CliError> {
        self.execute(Method::GET, path, None)
    }

    pub fn post<B: Serialize, T: DeserializeOwned>(
        &mut self,
        path: &str,
        body: &B,
    ) -> Result<T, CliError> {
        let v = serde_json::to_value(body)?;
        self.execute(Method::POST, path, Some(&v))
    }

    pub fn patch<B: Serialize, T: DeserializeOwned>(
        &mut self,
        path: &str,
        body: &B,
    ) -> Result<T, CliError> {
        let v = serde_json::to_value(body)?;
        self.execute(Method::PATCH, path, Some(&v))
    }

    pub fn put<B: Serialize, T: DeserializeOwned>(
        &mut self,
        path: &str,
        body: &B,
    ) -> Result<T, CliError> {
        let v = serde_json::to_value(body)?;
        self.execute(Method::PUT, path, Some(&v))
    }

    pub fn delete<T: DeserializeOwned>(&mut self, path: &str) -> Result<T, CliError> {
        self.execute(Method::DELETE, path, None)
    }
}

fn parse_envelope<T: DeserializeOwned>(resp: Response) -> Result<T, CliError> {
    let status = resp.status();
    let bytes = resp.bytes()?;
    let env: Envelope<T> = serde_json::from_slice(&bytes).map_err(|e| {
        // Include a snippet so users can see what the server returned if it
        // didn't speak envelope.
        let snippet = String::from_utf8_lossy(&bytes);
        let snippet = snippet.chars().take(200).collect::<String>();
        CliError::Decode(format!("{e} (body: {snippet:?})"))
    })?;

    if let Some(errors) = env.errors {
        return Err(CliError::Api {
            status: status.as_u16(),
            errors,
        });
    }
    env.data.ok_or(CliError::EmptyResponse(status.as_u16()))
}

fn expiry_from_ttl(secs: i64) -> Option<i64> {
    if secs <= 0 {
        return None;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .ok()?;
    Some(now + secs)
}

/// Helper: treat the TokenPair the server returned from `/auth/login` or
/// `/auth/register` as a fresh session and persist it.
pub fn save_session(
    api_url: &str,
    username: &str,
    pair: &SessionTokens,
) -> Result<Credentials, CliError> {
    let creds = Credentials {
        api_url: api_url.to_string(),
        username: username.to_string(),
        access_token: pair.access_token.clone(),
        refresh_token: pair.refresh_token.clone(),
        stay_logged_in: pair.stay_logged_in,
        access_expires_at: expiry_from_ttl(pair.access_expires_in),
        refresh_expires_at: expiry_from_ttl(pair.refresh_expires_in),
    };
    creds.save()?;
    Ok(creds)
}

/// Re-deserializable shape for `/auth/login` + `/auth/register` token payloads.
#[derive(Debug, Clone, Deserialize)]
pub struct SessionTokens {
    pub access_token: String,
    pub refresh_token: String,
    #[allow(dead_code)]
    pub token_type: String,
    pub access_expires_in: i64,
    pub refresh_expires_in: i64,
    #[serde(default)]
    pub stay_logged_in: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserView {
    pub id: String,
    pub username: String,
    pub created_at: i64,
    #[serde(default)]
    pub last_login_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionView {
    pub user: UserView,
    pub tokens: SessionTokens,
}
