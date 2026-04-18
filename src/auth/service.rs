//! Core auth operations: register, login, refresh, logout, load user.
//!
//! Every function here returns a [`crate::api::ApiError`] on failure so
//! handlers can simply `?` them and get a correctly-shaped envelope.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mongodb::bson::{doc, oid::ObjectId};
use serde::Serialize;
use uuid::Uuid;

use crate::api::ApiError;
use crate::db::{
    models::{refresh_token::RefreshToken, user::User},
    MongoModel,
};

use super::{
    config::AuthConfig,
    jwt::{self, Claims, TokenType},
    password,
};

#[derive(Debug, Clone, Serialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
    pub access_expires_in: i64,
    pub refresh_expires_in: i64,
    pub stay_logged_in: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserView {
    pub id: String,
    pub username: String,
    pub created_at: i64,
    pub last_login_at: Option<i64>,
}

impl From<&User> for UserView {
    fn from(u: &User) -> Self {
        Self {
            id: u.id.map(|o| o.to_hex()).unwrap_or_default(),
            username: u.username.clone(),
            created_at: u.created_at,
            last_login_at: u.last_login_at,
        }
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn validate_username(u: &str) -> Result<(), ApiError> {
    if u.len() < 3 || u.len() > 32 {
        return Err(ApiError::validation("username must be 3-32 characters")
            .with_source(serde_json::json!({ "pointer": "/username" })));
    }
    if !u.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.') {
        return Err(ApiError::validation(
            "username may only contain letters, digits, '_', '-', '.'",
        )
        .with_source(serde_json::json!({ "pointer": "/username" })));
    }
    Ok(())
}

fn validate_password(p: &str) -> Result<(), ApiError> {
    if p.len() < 8 {
        return Err(ApiError::validation("password must be at least 8 characters")
            .with_source(serde_json::json!({ "pointer": "/password" })));
    }
    if p.len() > 256 {
        return Err(ApiError::validation("password is too long (max 256)")
            .with_source(serde_json::json!({ "pointer": "/password" })));
    }
    Ok(())
}

/// Registers a new user. Returns an [`ApiError::conflict`] if the username is taken.
pub async fn register(username: &str, plaintext_password: &str) -> Result<User, ApiError> {
    validate_username(username)?;
    validate_password(plaintext_password)?;

    let users = User::repository();
    if users
        .find_one(doc! { "username": username })
        .await?
        .is_some()
    {
        return Err(ApiError::conflict("username is already taken")
            .with_source(serde_json::json!({ "pointer": "/username" })));
    }

    let hash = password::hash_password(plaintext_password)?;
    let mut user = User {
        id: None,
        username: username.to_string(),
        password_hash: hash,
        created_at: now_secs(),
        last_login_at: None,
    };
    let result = users.insert_one(&user).await?;
    user.id = result.inserted_id.as_object_id();

    Ok(user)
}

/// Verifies credentials, stamps `last_login_at`, and mints a fresh token pair.
pub async fn login(
    cfg: &AuthConfig,
    username: &str,
    plaintext_password: &str,
    stay_logged_in: bool,
) -> Result<(User, TokenPair), ApiError> {
    let users = User::repository();
    let user = users
        .find_one(doc! { "username": username })
        .await?
        .ok_or_else(|| {
            ApiError::unauthorized("invalid username or password")
                .with_code("invalid_credentials", "Invalid credentials")
        })?;

    if !password::verify_password(plaintext_password, &user.password_hash) {
        return Err(ApiError::unauthorized("invalid username or password")
            .with_code("invalid_credentials", "Invalid credentials"));
    }

    let user_id = user
        .id
        .ok_or_else(|| ApiError::internal("user missing id"))?;

    let now = now_secs();
    users
        .update_by_id(user_id, doc! { "$set": { "last_login_at": now } })
        .await?;

    let pair = issue_pair(cfg, user_id, stay_logged_in, None).await?;
    Ok((user, pair))
}

/// Rotates a refresh token: validates + revokes the old, mints a new pair.
///
/// If the presented refresh token is valid-looking but already revoked, we
/// treat it as replay/theft and revoke **every** active refresh for that user.
pub async fn refresh(cfg: &AuthConfig, refresh_token: &str) -> Result<TokenPair, ApiError> {
    let claims = jwt::decode(cfg, refresh_token, TokenType::Refresh)?;
    let user_id = ObjectId::parse_str(&claims.sub)
        .map_err(|_| ApiError::unauthorized("invalid token subject"))?;

    let tokens = RefreshToken::repository();
    let record = tokens
        .find_by_id(claims.jti.clone())
        .await?
        .ok_or_else(|| {
            ApiError::unauthorized("refresh token not recognized")
                .with_code("invalid_token", "Invalid token")
        })?;

    let now = now_secs();

    if record.revoked_at.is_some() {
        revoke_all_for_user(user_id).await?;
        return Err(ApiError::unauthorized(
            "refresh token was already used; all sessions revoked",
        )
        .with_code("token_reuse_detected", "Token reuse detected"));
    }
    if record.expires_at <= now {
        return Err(ApiError::unauthorized("refresh token expired")
            .with_code("token_expired", "Token expired"));
    }

    let pair = issue_pair(cfg, user_id, record.stay_logged_in, None).await?;

    // Extract new jti from the pair we just issued so we can chain it.
    let new_claims = jwt::decode(cfg, &pair.refresh_token, TokenType::Refresh)?;
    tokens
        .update_by_id(
            record.id.clone(),
            doc! {
                "$set": {
                    "revoked_at": now,
                    "replaced_by": new_claims.jti,
                }
            },
        )
        .await?;

    Ok(pair)
}

/// Revokes a single refresh token (logout). Silent on unknown/expired tokens.
pub async fn logout(cfg: &AuthConfig, refresh_token: &str) -> Result<(), ApiError> {
    let claims = match jwt::decode(cfg, refresh_token, TokenType::Refresh) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    let now = now_secs();
    RefreshToken::repository()
        .update_by_id(
            claims.jti,
            doc! { "$set": { "revoked_at": now } },
        )
        .await?;
    Ok(())
}

pub async fn find_user_by_id(user_id: ObjectId) -> Result<User, ApiError> {
    User::repository()
        .find_by_id(user_id)
        .await?
        .ok_or_else(|| ApiError::unauthorized("user no longer exists"))
}

async fn revoke_all_for_user(user_id: ObjectId) -> Result<(), ApiError> {
    let now = now_secs();
    RefreshToken::repository()
        .collection()
        .update_many(
            doc! { "user_id": user_id, "revoked_at": { "$exists": false } },
            doc! { "$set": { "revoked_at": now } },
        )
        .await?;
    Ok(())
}

async fn issue_pair(
    cfg: &AuthConfig,
    user_id: ObjectId,
    stay_logged_in: bool,
    link_from: Option<String>,
) -> Result<TokenPair, ApiError> {
    let now = now_secs();
    let access_ttl = cfg.access_ttl(stay_logged_in);
    let refresh_ttl = cfg.refresh_ttl(stay_logged_in);

    let access_jti = Uuid::new_v4().to_string();
    let refresh_jti = Uuid::new_v4().to_string();

    let access_claims = Claims {
        sub: user_id.to_hex(),
        exp: now + access_ttl.as_secs() as i64,
        iat: now,
        iss: cfg.issuer.clone(),
        jti: access_jti,
        typ: TokenType::Access,
        sli: stay_logged_in,
    };
    let refresh_claims = Claims {
        sub: user_id.to_hex(),
        exp: now + refresh_ttl.as_secs() as i64,
        iat: now,
        iss: cfg.issuer.clone(),
        jti: refresh_jti.clone(),
        typ: TokenType::Refresh,
        sli: stay_logged_in,
    };

    let access_token = jwt::encode(cfg, &access_claims)?;
    let refresh_token = jwt::encode(cfg, &refresh_claims)?;

    let record = RefreshToken {
        id: refresh_jti,
        user_id,
        issued_at: now,
        expires_at: refresh_claims.exp,
        revoked_at: None,
        replaced_by: link_from,
        stay_logged_in,
    };
    RefreshToken::repository().insert_one(&record).await?;

    Ok(TokenPair {
        access_token,
        refresh_token,
        token_type: "Bearer",
        access_expires_in: access_ttl.as_secs() as i64,
        refresh_expires_in: refresh_ttl.as_secs() as i64,
        stay_logged_in,
    })
}

/// Small helper for tests / ad-hoc minting.
#[allow(dead_code)]
pub fn new_uuid_ttl(d: Duration) -> (String, i64) {
    (Uuid::new_v4().to_string(), now_secs() + d.as_secs() as i64)
}
