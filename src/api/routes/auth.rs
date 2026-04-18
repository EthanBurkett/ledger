use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::api::{ApiError, ApiResult, RequestContext};
use crate::app::App;
use crate::auth::{service, AuthUser, TokenPair, UserView};

#[derive(Deserialize)]
struct CredentialsBody {
    username: String,
    password: String,
    #[serde(default)]
    stay_logged_in: bool,
}

#[derive(Deserialize)]
struct RegisterBody {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct RefreshBody {
    refresh_token: String,
}

#[derive(Deserialize)]
struct LogoutBody {
    refresh_token: String,
}

#[derive(Serialize)]
struct Session {
    user: UserView,
    tokens: TokenPair,
}

async fn register(
    ctx: RequestContext,
    Json(body): Json<RegisterBody>,
) -> ApiResult<Session> {
    let cfg = App::get().auth_config();
    let user = service::register(body.username.trim(), &body.password).await?;
    let user_id = user.id.ok_or_else(|| ApiError::internal("user missing id"))?;
    // Register auto-logs-in with default (non-persistent) session.
    let pair = {
        let (_, pair) = service::login(cfg, &user.username, &body.password, false).await?;
        pair
    };
    let _ = user_id; // used only for clarity; service::login reloads the user
    Ok(ctx.respond(
        StatusCode::CREATED,
        Session {
            user: UserView::from(&user),
            tokens: pair,
        },
    ))
}

async fn login(
    ctx: RequestContext,
    Json(body): Json<CredentialsBody>,
) -> ApiResult<Session> {
    let cfg = App::get().auth_config();
    let (user, pair) = service::login(
        cfg,
        body.username.trim(),
        &body.password,
        body.stay_logged_in,
    )
    .await?;
    Ok(ctx.ok(Session {
        user: UserView::from(&user),
        tokens: pair,
    }))
}

async fn refresh(
    ctx: RequestContext,
    Json(body): Json<RefreshBody>,
) -> ApiResult<TokenPair> {
    let cfg = App::get().auth_config();
    let pair = service::refresh(cfg, &body.refresh_token).await?;
    Ok(ctx.ok(pair))
}

#[derive(Serialize)]
struct LoggedOut {
    logged_out: bool,
}

async fn logout(
    ctx: RequestContext,
    Json(body): Json<LogoutBody>,
) -> ApiResult<LoggedOut> {
    let cfg = App::get().auth_config();
    service::logout(cfg, &body.refresh_token).await?;
    Ok(ctx.ok(LoggedOut { logged_out: true }))
}

async fn me(ctx: RequestContext, auth: AuthUser) -> ApiResult<UserView> {
    Ok(ctx.ok(UserView::from(&auth.user)))
}

fn register_routes(router: Router) -> Router {
    router
        .route("/v1/auth/register", post(register))
        .route("/v1/auth/login", post(login))
        .route("/v1/auth/refresh", post(refresh))
        .route("/v1/auth/logout", post(logout))
        .route("/v1/auth/me", get(me))
}

crate::register_routes!(register_routes);
