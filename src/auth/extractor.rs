//! `AuthUser` extractor: pulls and validates a `Bearer <access_token>` header.
//!
//! ```ignore
//! async fn handler(ctx: RequestContext, auth: AuthUser) -> ApiResult<MeView> {
//!     Ok(ctx.ok(MeView::from(&auth.user)))
//! }
//! ```

use axum::{extract::FromRequestParts, http::request::Parts};
use mongodb::bson::oid::ObjectId;

use crate::api::ApiError;
use crate::app::App;
use crate::db::models::user::User;

use super::{
    jwt::{self, Claims, TokenType},
    service,
};

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user: User,
    #[allow(dead_code)]
    pub claims: Claims,
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                ApiError::unauthorized("missing Authorization header")
                    .with_code("missing_token", "Missing token")
            })?;

        let token = header.strip_prefix("Bearer ").ok_or_else(|| {
            ApiError::unauthorized("Authorization header must use Bearer scheme")
                .with_code("invalid_scheme", "Invalid auth scheme")
        })?;

        let app = App::get();
        let claims = jwt::decode(app.auth_config(), token, TokenType::Access)?;
        let user_id = ObjectId::parse_str(&claims.sub)
            .map_err(|_| ApiError::unauthorized("invalid token subject"))?;

        let user = service::find_user_by_id(user_id).await?;
        Ok(AuthUser { user, claims })
    }
}
