//! JWT claims + encoding/decoding.

use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::api::ApiError;

use super::config::AuthConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenType {
    Access,
    Refresh,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject: user id as hex ObjectId string.
    pub sub: String,
    /// Expiration (unix seconds).
    pub exp: i64,
    /// Issued at (unix seconds).
    pub iat: i64,
    /// Issuer.
    pub iss: String,
    /// Token id. For refresh tokens this matches a DB record.
    pub jti: String,
    /// Token kind.
    pub typ: TokenType,
    /// Whether this session was issued with the "stay logged in" flag.
    pub sli: bool,
}

pub fn encode(cfg: &AuthConfig, claims: &Claims) -> Result<String, ApiError> {
    let key = EncodingKey::from_secret(cfg.secret());
    jsonwebtoken::encode(&Header::default(), claims, &key)
        .map_err(|e| ApiError::internal(format!("jwt encode failed: {e}")))
}

pub fn decode(cfg: &AuthConfig, token: &str, expected: TokenType) -> Result<Claims, ApiError> {
    let key = DecodingKey::from_secret(cfg.secret());
    let mut validation = Validation::default();
    validation.set_issuer(&[&cfg.issuer]);

    let data = jsonwebtoken::decode::<Claims>(token, &key, &validation).map_err(|e| {
        use jsonwebtoken::errors::ErrorKind;
        match e.kind() {
            ErrorKind::ExpiredSignature => {
                ApiError::unauthorized("token expired").with_code("token_expired", "Token expired")
            }
            ErrorKind::InvalidSignature | ErrorKind::InvalidToken => {
                ApiError::unauthorized("invalid token")
                    .with_code("invalid_token", "Invalid token")
            }
            _ => ApiError::unauthorized(format!("token validation failed: {e}"))
                .with_code("invalid_token", "Invalid token"),
        }
    })?;

    if data.claims.typ != expected {
        return Err(ApiError::unauthorized(format!(
            "expected {expected:?} token, got {:?}",
            data.claims.typ
        ))
        .with_code("wrong_token_type", "Wrong token type"));
    }

    Ok(data.claims)
}
