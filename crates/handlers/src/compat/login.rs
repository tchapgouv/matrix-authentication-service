// Copyright 2024 New Vector Ltd.
// Copyright 2022-2024 The Matrix.org Foundation C.I.C.
//
// SPDX-License-Identifier: AGPL-3.0-only
// Please see LICENSE in the repository root for full details.

use std::sync::Arc;

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    response::IntoResponse,
};
use axum_extra::{extract::WithRejection, typed_header::TypedHeader};
use chrono::Duration;
use hyper::StatusCode;
use mas_axum_utils::sentry::SentryEventID;
use mas_data_model::{
    CompatSession, CompatSsoLoginState, Device, SiteConfig, TokenType, User, UserAgent,
};
use mas_matrix::HomeserverConnection;
use mas_storage::{
    BoxClock, BoxRepository, BoxRng, Clock, RepositoryAccess,
    compat::{
        CompatAccessTokenRepository, CompatRefreshTokenRepository, CompatSessionRepository,
        CompatSsoLoginRepository,
    },
    user::{UserPasswordRepository, UserRepository},
};
use rand::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_with::{DurationMilliSeconds, serde_as, skip_serializing_none};
use thiserror::Error;
use zeroize::Zeroizing;

use super::MatrixError;
use crate::{
    BoundActivityTracker, Limiter, RequesterFingerprint, impl_from_error_for_route,
    passwords::PasswordManager, rate_limit::PasswordCheckLimitedError,
};

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum LoginType {
    #[serde(rename = "m.login.password")]
    Password,

    // we will leave MSC3824 `actions` as undefined for this auth type as unclear
    // how it should be interpreted
    #[serde(rename = "m.login.token")]
    Token,

    #[serde(rename = "m.login.sso")]
    Sso {
        #[serde(skip_serializing_if = "Vec::is_empty")]
        identity_providers: Vec<SsoIdentityProvider>,
        #[serde(rename = "org.matrix.msc3824.delegated_oidc_compatibility")]
        delegated_oidc_compatibility: bool,
    },
}

#[derive(Debug, Serialize)]
struct SsoIdentityProvider {
    id: &'static str,
    name: &'static str,
}

#[derive(Debug, Serialize)]
struct LoginTypes {
    flows: Vec<LoginType>,
}

#[tracing::instrument(name = "handlers.compat.login.get", skip_all)]
pub(crate) async fn get(State(password_manager): State<PasswordManager>) -> impl IntoResponse {
    let flows = if password_manager.is_enabled() {
        vec![
            LoginType::Password,
            LoginType::Sso {
                identity_providers: vec![],
                delegated_oidc_compatibility: true,
            },
            LoginType::Token,
        ]
    } else {
        vec![
            LoginType::Sso {
                identity_providers: vec![],
                delegated_oidc_compatibility: true,
            },
            LoginType::Token,
        ]
    };

    let res = LoginTypes { flows };

    Json(res)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestBody {
    #[serde(flatten)]
    credentials: Credentials,

    #[serde(default)]
    refresh_token: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Credentials {
    #[serde(rename = "m.login.password")]
    Password {
        identifier: Option<Identifier>,
        // This property has been deprecated for a while, but some tools still use it.
        user: Option<String>,
        password: String,
    },

    #[serde(rename = "m.login.token")]
    Token { token: String },

    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Identifier {
    #[serde(rename = "m.id.user")]
    User { user: String },

    #[serde(other)]
    Unsupported,
}

#[skip_serializing_none]
#[serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseBody {
    access_token: String,
    device_id: Option<Device>,
    user_id: String,
    refresh_token: Option<String>,
    #[serde_as(as = "Option<DurationMilliSeconds<i64>>")]
    expires_in_ms: Option<Duration>,
}

#[derive(Debug, Error)]
pub enum RouteError {
    #[error(transparent)]
    Internal(Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error("unsupported login method")]
    Unsupported,

    #[error("unsupported identifier type")]
    UnsupportedIdentifier,

    #[error("missing property 'identifier'")]
    MissingIdentifier,

    #[error("user not found")]
    UserNotFound,

    #[error("session not found")]
    SessionNotFound,

    #[error("user has no password")]
    NoPassword,

    #[error("password verification failed")]
    PasswordVerificationFailed(#[source] anyhow::Error),

    #[error("request rate limited")]
    RateLimited(#[from] PasswordCheckLimitedError),

    #[error("login took too long")]
    LoginTookTooLong,

    #[error("invalid login token")]
    InvalidLoginToken,

    #[error(transparent)]
    InvalidJsonBody(#[from] JsonRejection),

    #[error("failed to provision device")]
    ProvisionDeviceFailed(#[source] anyhow::Error),
}

impl_from_error_for_route!(mas_storage::RepositoryError);

impl IntoResponse for RouteError {
    fn into_response(self) -> axum::response::Response {
        let event_id = sentry::capture_error(&self);
        let response = match self {
            Self::Internal(_) | Self::SessionNotFound | Self::ProvisionDeviceFailed(_) => {
                MatrixError {
                    errcode: "M_UNKNOWN",
                    error: "Internal server error",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            }
            Self::RateLimited(_) => MatrixError {
                errcode: "M_LIMIT_EXCEEDED",
                error: "Too many login attempts",
                status: StatusCode::TOO_MANY_REQUESTS,
            },
            Self::InvalidJsonBody(JsonRejection::MissingJsonContentType(_)) => MatrixError {
                errcode: "M_NOT_JSON",
                error: "Invalid Content-Type header: expected application/json",
                status: StatusCode::BAD_REQUEST,
            },
            Self::InvalidJsonBody(JsonRejection::JsonSyntaxError(_)) => MatrixError {
                errcode: "M_NOT_JSON",
                error: "Body is not a valid JSON document",
                status: StatusCode::BAD_REQUEST,
            },
            Self::InvalidJsonBody(JsonRejection::JsonDataError(_)) => MatrixError {
                errcode: "M_BAD_JSON",
                error: "JSON fields are not valid",
                status: StatusCode::BAD_REQUEST,
            },
            Self::InvalidJsonBody(_) => MatrixError {
                errcode: "M_UNKNOWN",
                error: "Unknown error while parsing JSON body",
                status: StatusCode::BAD_REQUEST,
            },
            Self::Unsupported => MatrixError {
                errcode: "M_UNKNOWN",
                error: "Invalid login type",
                status: StatusCode::BAD_REQUEST,
            },
            Self::UnsupportedIdentifier => MatrixError {
                errcode: "M_UNKNOWN",
                error: "Unsupported login identifier",
                status: StatusCode::BAD_REQUEST,
            },
            Self::MissingIdentifier => MatrixError {
                errcode: "M_BAD_JSON",
                error: "Missing property 'identifier",
                status: StatusCode::BAD_REQUEST,
            },
            Self::UserNotFound | Self::NoPassword | Self::PasswordVerificationFailed(_) => {
                MatrixError {
                    errcode: "M_FORBIDDEN",
                    error: "Invalid username/password",
                    status: StatusCode::FORBIDDEN,
                }
            }
            Self::LoginTookTooLong => MatrixError {
                errcode: "M_FORBIDDEN",
                error: "Login token expired",
                status: StatusCode::FORBIDDEN,
            },
            Self::InvalidLoginToken => MatrixError {
                errcode: "M_FORBIDDEN",
                error: "Invalid login token",
                status: StatusCode::FORBIDDEN,
            },
        };

        (SentryEventID::from(event_id), response).into_response()
    }
}

#[tracing::instrument(name = "handlers.compat.login.post", skip_all, err)]
pub(crate) async fn post(
    mut rng: BoxRng,
    clock: BoxClock,
    State(password_manager): State<PasswordManager>,
    mut repo: BoxRepository,
    activity_tracker: BoundActivityTracker,
    State(homeserver): State<Arc<dyn HomeserverConnection>>,
    State(site_config): State<SiteConfig>,
    State(limiter): State<Limiter>,
    requester: RequesterFingerprint,
    user_agent: Option<TypedHeader<headers::UserAgent>>,
    WithRejection(Json(input), _): WithRejection<Json<RequestBody>, RouteError>,
) -> Result<impl IntoResponse, RouteError> {
    let user_agent = user_agent.map(|ua| UserAgent::parse(ua.as_str().to_owned()));
    let (mut session, user) = match (password_manager.is_enabled(), input.credentials) {
        (
            true,
            Credentials::Password {
                identifier,
                user,
                password,
            },
        ) => {
            // This is to support both the (very) old and deprecated 'user' property, with
            // the same behavior as Synapse: it takes precendence over the 'identifier' if
            // provided
            let user = match (identifier, user) {
                (Some(Identifier::User { user }), None) | (_, Some(user)) => user,
                (Some(Identifier::Unsupported), None) => {
                    return Err(RouteError::UnsupportedIdentifier);
                }
                (None, None) => {
                    return Err(RouteError::MissingIdentifier);
                }
            };

            user_password_login(
                &mut rng,
                &clock,
                &password_manager,
                &limiter,
                requester,
                &mut repo,
                &homeserver,
                user,
                password,
            )
            .await?
        }

        (_, Credentials::Token { token }) => token_login(&mut repo, &clock, &token).await?,

        _ => {
            return Err(RouteError::Unsupported);
        }
    };

    if let Some(user_agent) = user_agent {
        session = repo
            .compat_session()
            .record_user_agent(session, user_agent)
            .await?;
    }

    let user_id = homeserver.mxid(&user.username);

    // If the client asked for a refreshable token, make it expire
    let expires_in = if input.refresh_token {
        Some(site_config.compat_token_ttl)
    } else {
        None
    };

    let access_token = TokenType::CompatAccessToken.generate(&mut rng);
    let access_token = repo
        .compat_access_token()
        .add(&mut rng, &clock, &session, access_token, expires_in)
        .await?;

    let refresh_token = if input.refresh_token {
        let refresh_token = TokenType::CompatRefreshToken.generate(&mut rng);
        let refresh_token = repo
            .compat_refresh_token()
            .add(&mut rng, &clock, &session, &access_token, refresh_token)
            .await?;
        Some(refresh_token.token)
    } else {
        None
    };

    repo.save().await?;

    activity_tracker
        .record_compat_session(&clock, &session)
        .await;

    Ok(Json(ResponseBody {
        access_token: access_token.token,
        device_id: session.device,
        user_id,
        refresh_token,
        expires_in_ms: expires_in,
    }))
}

async fn token_login(
    repo: &mut BoxRepository,
    clock: &dyn Clock,
    token: &str,
) -> Result<(CompatSession, User), RouteError> {
    let login = repo
        .compat_sso_login()
        .find_by_token(token)
        .await?
        .ok_or(RouteError::InvalidLoginToken)?;

    let now = clock.now();
    let session_id = match login.state {
        CompatSsoLoginState::Pending => {
            tracing::error!(
                compat_sso_login.id = %login.id,
                "Exchanged a token for a login that was not fullfilled yet"
            );
            return Err(RouteError::InvalidLoginToken);
        }
        CompatSsoLoginState::Fulfilled {
            fulfilled_at,
            session_id,
            ..
        } => {
            if now > fulfilled_at + Duration::microseconds(30 * 1000 * 1000) {
                return Err(RouteError::LoginTookTooLong);
            }

            session_id
        }
        CompatSsoLoginState::Exchanged {
            exchanged_at,
            session_id,
            ..
        } => {
            if now > exchanged_at + Duration::microseconds(30 * 1000 * 1000) {
                // TODO: log that session out
                tracing::error!(
                    compat_sso_login.id = %login.id,
                    compat_session.id = %session_id,
                    "Login token exchanged a second time more than 30s after"
                );
            }

            return Err(RouteError::InvalidLoginToken);
        }
    };

    let session = repo
        .compat_session()
        .lookup(session_id)
        .await?
        .ok_or(RouteError::SessionNotFound)?;

    let user = repo
        .user()
        .lookup(session.user_id)
        .await?
        .filter(mas_data_model::User::is_valid)
        .ok_or(RouteError::UserNotFound)?;

    repo.compat_sso_login().exchange(clock, login).await?;

    Ok((session, user))
}

async fn user_password_login(
    mut rng: &mut (impl RngCore + CryptoRng + Send),
    clock: &impl Clock,
    password_manager: &PasswordManager,
    limiter: &Limiter,
    requester: RequesterFingerprint,
    repo: &mut BoxRepository,
    homeserver: &dyn HomeserverConnection,
    username: String,
    password: String,
) -> Result<(CompatSession, User), RouteError> {
    // Try getting the localpart out of the MXID
    let username = homeserver.localpart(&username).unwrap_or(&username);

    // Find the user
    let user = repo
        .user()
        .find_by_username(username)
        .await?
        .filter(mas_data_model::User::is_valid)
        .ok_or(RouteError::UserNotFound)?;

    // Check the rate limit
    limiter.check_password(requester, &user)?;

    // Lookup its password
    let user_password = repo
        .user_password()
        .active(&user)
        .await?
        .ok_or(RouteError::NoPassword)?;

    // Verify the password
    let password = Zeroizing::new(password.into_bytes());

    let new_password_hash = password_manager
        .verify_and_upgrade(
            &mut rng,
            user_password.version,
            password,
            user_password.hashed_password.clone(),
        )
        .await
        .map_err(RouteError::PasswordVerificationFailed)?;

    if let Some((version, hashed_password)) = new_password_hash {
        // Save the upgraded password if needed
        repo.user_password()
            .add(
                &mut rng,
                clock,
                &user,
                version,
                hashed_password,
                Some(&user_password),
            )
            .await?;
    }

    // Lock the user sync to make sure we don't get into a race condition
    repo.user().acquire_lock_for_sync(&user).await?;

    // Now that the user credentials have been verified, start a new compat session
    let device = Device::generate(&mut rng);
    let mxid = homeserver.mxid(&user.username);
    homeserver
        .create_device(&mxid, device.as_str())
        .await
        .map_err(RouteError::ProvisionDeviceFailed)?;

    let session = repo
        .compat_session()
        .add(&mut rng, clock, &user, device, None, false)
        .await?;

    Ok((session, user))
}

#[cfg(test)]
mod tests {
    use hyper::Request;
    use mas_matrix::{HomeserverConnection, ProvisionRequest};
    use rand::distributions::{Alphanumeric, DistString};
    use sqlx::PgPool;

    use super::*;
    use crate::test_utils::{RequestBuilderExt, ResponseExt, TestState, setup, test_site_config};

    /// Test that the server advertises the right login flows.
    #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    async fn test_get_login(pool: PgPool) {
        setup();
        let state = TestState::from_pool(pool).await.unwrap();

        // Now let's get the login flows
        let request = Request::get("/_matrix/client/v3/login").empty();
        let response = state.request(request).await;
        response.assert_status(StatusCode::OK);
        let body: serde_json::Value = response.json();

        insta::assert_json_snapshot!(body, @r###"
        {
          "flows": [
            {
              "type": "m.login.password"
            },
            {
              "type": "m.login.sso",
              "org.matrix.msc3824.delegated_oidc_compatibility": true
            },
            {
              "type": "m.login.token"
            }
          ]
        }
        "###);
    }

    /// Test the cases where the body is invalid
    #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    async fn test_bad_body(pool: PgPool) {
        setup();
        let state = TestState::from_pool(pool).await.unwrap();

        // No/empty body
        let request = Request::post("/_matrix/client/v3/login").empty();
        let response = state.request(request).await;
        response.assert_status(StatusCode::BAD_REQUEST);
        let body: serde_json::Value = response.json();

        insta::assert_json_snapshot!(body, @r###"
        {
          "errcode": "M_NOT_JSON",
          "error": "Invalid Content-Type header: expected application/json"
        }
        "###);

        // Missing keys in body
        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({}));
        let response = state.request(request).await;
        response.assert_status(StatusCode::BAD_REQUEST);
        let body: serde_json::Value = response.json();

        insta::assert_json_snapshot!(body, @r###"
        {
          "errcode": "M_BAD_JSON",
          "error": "JSON fields are not valid"
        }
        "###);

        // Invalid JSON
        let request = Request::post("/_matrix/client/v3/login")
            .header("Content-Type", "application/json")
            .body("{".to_owned())
            .unwrap();
        let response = state.request(request).await;
        response.assert_status(StatusCode::BAD_REQUEST);
        let body: serde_json::Value = response.json();

        insta::assert_json_snapshot!(body, @r###"
        {
          "errcode": "M_NOT_JSON",
          "error": "Body is not a valid JSON document"
        }
        "###);
    }

    /// Test that the server doesn't allow login with a password if the password
    /// manager is disabled
    #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    async fn test_password_disabled(pool: PgPool) {
        setup();
        let state = TestState::from_pool_with_site_config(
            pool,
            SiteConfig {
                password_login_enabled: false,
                ..test_site_config()
            },
        )
        .await
        .unwrap();

        // Now let's get the login flows
        let request = Request::get("/_matrix/client/v3/login").empty();
        let response = state.request(request).await;
        response.assert_status(StatusCode::OK);
        let body: serde_json::Value = response.json();

        insta::assert_json_snapshot!(body, @r###"
        {
          "flows": [
            {
              "type": "m.login.sso",
              "org.matrix.msc3824.delegated_oidc_compatibility": true
            },
            {
              "type": "m.login.token"
            }
          ]
        }
        "###);

        // Try to login with a password, it should be rejected
        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({
            "type": "m.login.password",
            "identifier": {
                "type": "m.id.user",
                "user": "alice",
            },
            "password": "password",
        }));

        let response = state.request(request).await;
        response.assert_status(StatusCode::BAD_REQUEST);
        let body: serde_json::Value = response.json();
        insta::assert_json_snapshot!(body, @r###"
        {
          "errcode": "M_UNKNOWN",
          "error": "Invalid login type"
        }
        "###);
    }

    async fn user_with_password(state: &TestState, username: &str, password: &str) {
        let mut rng = state.rng();
        let mut repo = state.repository().await.unwrap();

        let user = repo
            .user()
            .add(&mut rng, &state.clock, username.to_owned())
            .await
            .unwrap();
        let (version, hash) = state
            .password_manager
            .hash(&mut rng, Zeroizing::new(password.as_bytes().to_vec()))
            .await
            .unwrap();

        repo.user_password()
            .add(&mut rng, &state.clock, &user, version, hash, None)
            .await
            .unwrap();
        let mxid = state.homeserver_connection.mxid(&user.username);
        state
            .homeserver_connection
            .provision_user(&ProvisionRequest::new(mxid, &user.sub))
            .await
            .unwrap();

        repo.save().await.unwrap();
    }

    /// Test that a user can login with a password using the Matrix
    /// compatibility API.
    #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    async fn test_user_password_login(pool: PgPool) {
        setup();
        let state = TestState::from_pool(pool).await.unwrap();

        user_with_password(&state, "alice", "password").await;

        // Now let's try to login with the password, without asking for a refresh token.
        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({
            "type": "m.login.password",
            "identifier": {
                "type": "m.id.user",
                "user": "alice",
            },
            "password": "password",
        }));

        let response = state.request(request).await;
        response.assert_status(StatusCode::OK);

        let body: serde_json::Value = response.json();
        insta::assert_json_snapshot!(body, @r###"
        {
          "access_token": "mct_16tugBE5Ta9LIWoSJaAEHHq2g3fx8S_alcBB4",
          "device_id": "ZGpSvYQqlq",
          "user_id": "@alice:example.com"
        }
        "###);

        // Do the same, but this time ask for a refresh token.
        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({
            "type": "m.login.password",
            "identifier": {
                "type": "m.id.user",
                "user": "alice",
            },
            "password": "password",
            "refresh_token": true,
        }));

        let response = state.request(request).await;
        response.assert_status(StatusCode::OK);

        let body: serde_json::Value = response.json();
        insta::assert_json_snapshot!(body, @r###"
        {
          "access_token": "mct_cxG6gZXyvelQWW9XqfNbm5KAQovodf_XvJz43",
          "device_id": "42oTpLoieH",
          "user_id": "@alice:example.com",
          "refresh_token": "mcr_7IvDc44woP66fRQoS9MVcHXO9OeBmR_0jDGr1",
          "expires_in_ms": 300000
        }
        "###);

        // Try logging in with the 'user' property
        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({
            "type": "m.login.password",
            "user": "alice",
            "password": "password",
        }));

        let response = state.request(request).await;
        response.assert_status(StatusCode::OK);

        let body: serde_json::Value = response.json();
        insta::assert_json_snapshot!(body, @r###"
        {
          "access_token": "mct_PGMLvvMXC4Ds1A3lCWc6Hx4l9DGzqG_lVEIV2",
          "device_id": "Yp7FM44zJN",
          "user_id": "@alice:example.com"
        }
        "###);

        // Reset the state, to reset rate limits
        let state = state.reset().await;

        // Try to login with a wrong password.
        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({
            "type": "m.login.password",
            "identifier": {
                "type": "m.id.user",
                "user": "alice",
            },
            "password": "wrongpassword",
        }));

        let response = state.request(request).await;
        response.assert_status(StatusCode::FORBIDDEN);
        let body: serde_json::Value = response.json();
        insta::assert_json_snapshot!(body, @r###"
        {
          "errcode": "M_FORBIDDEN",
          "error": "Invalid username/password"
        }
        "###);

        // Try to login with a wrong username.
        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({
            "type": "m.login.password",
            "identifier": {
                "type": "m.id.user",
                "user": "bob",
            },
            "password": "wrongpassword",
        }));

        let old_body = body;
        let response = state.request(request).await;
        response.assert_status(StatusCode::FORBIDDEN);
        let body: serde_json::Value = response.json();

        // The response should be the same as the previous one, so that we don't leak if
        // it's the user that is invalid or the password.
        assert_eq!(body, old_body);
    }

    /// Test that a user can login with a password using the Matrix
    /// compatibility API, using a MXID as identifier
    #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    async fn test_user_password_login_mxid(pool: PgPool) {
        setup();
        let state = TestState::from_pool(pool).await.unwrap();

        user_with_password(&state, "alice", "password").await;

        // Login with a full MXID as identifier
        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({
            "type": "m.login.password",
            "identifier": {
                "type": "m.id.user",
                "user": "@alice:example.com",
            },
            "password": "password",
        }));

        let response = state.request(request).await;
        response.assert_status(StatusCode::OK);
        let body: serde_json::Value = response.json();
        insta::assert_json_snapshot!(body, @r###"
        {
          "access_token": "mct_16tugBE5Ta9LIWoSJaAEHHq2g3fx8S_alcBB4",
          "device_id": "ZGpSvYQqlq",
          "user_id": "@alice:example.com"
        }
        "###);

        // With a MXID, but with the wrong server name
        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({
            "type": "m.login.password",
            "identifier": {
                "type": "m.id.user",
                "user": "@alice:something.corp",
            },
            "password": "password",
        }));

        let response = state.request(request).await;
        response.assert_status(StatusCode::FORBIDDEN);
        let body: serde_json::Value = response.json();
        insta::assert_json_snapshot!(body, @r###"
        {
          "errcode": "M_FORBIDDEN",
          "error": "Invalid username/password"
        }
        "###);
    }

    /// Test that password logins are rate limited.
    #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    async fn test_password_login_rate_limit(pool: PgPool) {
        setup();
        let state = TestState::from_pool(pool).await.unwrap();

        // Let's provision a user without a password. This should be enough to trigger
        // the rate limit.
        let mut repo = state.repository().await.unwrap();

        let user = repo
            .user()
            .add(&mut state.rng(), &state.clock, "alice".to_owned())
            .await
            .unwrap();

        let mxid = state.homeserver_connection.mxid(&user.username);
        state
            .homeserver_connection
            .provision_user(&ProvisionRequest::new(mxid, &user.sub))
            .await
            .unwrap();

        repo.save().await.unwrap();

        // Now let's try to login with the password, without asking for a refresh token.
        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({
            "type": "m.login.password",
            "identifier": {
                "type": "m.id.user",
                "user": "alice",
            },
            "password": "password",
        }));

        // First three attempts should just tell about the invalid credentials
        let response = state.request(request.clone()).await;
        response.assert_status(StatusCode::FORBIDDEN);
        let response = state.request(request.clone()).await;
        response.assert_status(StatusCode::FORBIDDEN);
        let response = state.request(request.clone()).await;
        response.assert_status(StatusCode::FORBIDDEN);

        // The fourth attempt should be rate limited
        let response = state.request(request.clone()).await;
        response.assert_status(StatusCode::TOO_MANY_REQUESTS);
        let body: serde_json::Value = response.json();
        insta::assert_json_snapshot!(body, @r###"
        {
          "errcode": "M_LIMIT_EXCEEDED",
          "error": "Too many login attempts"
        }
        "###);
    }

    /// Test the response of an unsupported password identifier.
    #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    async fn test_unsupported_login_identifier(pool: PgPool) {
        setup();
        let state = TestState::from_pool(pool).await.unwrap();

        // Try to login with an unsupported login flow.
        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({
            "type": "m.login.password",
            "identifier": {
                "type": "m.id.email",
                "user": "user@example.com"
            },
            "password": "password"
        }));

        let response = state.request(request).await;
        response.assert_status(StatusCode::BAD_REQUEST);
        let body: serde_json::Value = response.json();
        insta::assert_json_snapshot!(body, @r###"
        {
          "errcode": "M_UNKNOWN",
          "error": "Unsupported login identifier"
        }
        "###);
    }

    /// Test the response of an unsupported login flow.
    #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    async fn test_unsupported_login(pool: PgPool) {
        setup();
        let state = TestState::from_pool(pool).await.unwrap();

        // Try to login with an unsupported login flow.
        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({
            "type": "m.login.unsupported",
        }));

        let response = state.request(request).await;
        response.assert_status(StatusCode::BAD_REQUEST);
        let body: serde_json::Value = response.json();
        insta::assert_json_snapshot!(body, @r###"
        {
          "errcode": "M_UNKNOWN",
          "error": "Invalid login type"
        }
        "###);
    }

    /// Test `m.login.token` login flow.
    #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    async fn test_login_token_login(pool: PgPool) {
        setup();
        let state = TestState::from_pool(pool).await.unwrap();

        // Provision a user
        let mut repo = state.repository().await.unwrap();

        let user = repo
            .user()
            .add(&mut state.rng(), &state.clock, "alice".to_owned())
            .await
            .unwrap();
        repo.save().await.unwrap();

        let mxid = state.homeserver_connection.mxid(&user.username);
        state
            .homeserver_connection
            .provision_user(&ProvisionRequest::new(mxid, &user.sub))
            .await
            .unwrap();

        // First try with an invalid token
        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({
            "type": "m.login.token",
            "token": "someinvalidtoken",
        }));

        let response = state.request(request).await;
        response.assert_status(StatusCode::FORBIDDEN);
        let body: serde_json::Value = response.json();
        insta::assert_json_snapshot!(body, @r###"
        {
          "errcode": "M_FORBIDDEN",
          "error": "Invalid login token"
        }
        "###);

        let (device, token) = get_login_token(&state, &user).await;

        // Try to login with the token.
        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({
            "type": "m.login.token",
            "token": token,
        }));
        let response = state.request(request).await;
        response.assert_status(StatusCode::OK);

        let body: serde_json::Value = response.json();
        insta::assert_json_snapshot!(body, @r###"
        {
          "access_token": "mct_uihy4bk51gxgUbUTa4XIh92RARTPTj_xADEE4",
          "device_id": "Yp7FM44zJN",
          "user_id": "@alice:example.com"
        }
        "###);
        assert_eq!(body["device_id"], device.to_string());

        // Try again with the same token, it should fail.
        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({
            "type": "m.login.token",
            "token": token,
        }));
        let response = state.request(request).await;
        response.assert_status(StatusCode::FORBIDDEN);
        let body: serde_json::Value = response.json();
        insta::assert_json_snapshot!(body, @r###"
        {
          "errcode": "M_FORBIDDEN",
          "error": "Invalid login token"
        }
        "###);

        // Try to login, but wait too long before sending the request.
        let (_device, token) = get_login_token(&state, &user).await;

        // Advance the clock to make the token expire.
        state
            .clock
            .advance(Duration::microseconds(60 * 1000 * 1000));

        let request = Request::post("/_matrix/client/v3/login").json(serde_json::json!({
            "type": "m.login.token",
            "token": token,
        }));
        let response = state.request(request).await;
        response.assert_status(StatusCode::FORBIDDEN);
        let body: serde_json::Value = response.json();
        insta::assert_json_snapshot!(body, @r###"
        {
          "errcode": "M_FORBIDDEN",
          "error": "Login token expired"
        }
        "###);
    }

    /// Get a login token for a user.
    /// Returns the device and the token.
    ///
    /// # Panics
    ///
    /// Panics if the repository fails.
    async fn get_login_token(state: &TestState, user: &User) -> (Device, String) {
        // XXX: This is a bit manual, but this is what basically the SSO login flow
        // does.
        let mut repo = state.repository().await.unwrap();

        // Generate a device and a token randomly
        let token = Alphanumeric.sample_string(&mut state.rng(), 32);
        let device = Device::generate(&mut state.rng());

        // Start a compat SSO login flow
        let login = repo
            .compat_sso_login()
            .add(
                &mut state.rng(),
                &state.clock,
                token.clone(),
                "http://example.com/".parse().unwrap(),
            )
            .await
            .unwrap();

        // Complete the flow by fulfilling it with a session
        let compat_session = repo
            .compat_session()
            .add(
                &mut state.rng(),
                &state.clock,
                user,
                device.clone(),
                None,
                false,
            )
            .await
            .unwrap();

        repo.compat_sso_login()
            .fulfill(&state.clock, login, &compat_session)
            .await
            .unwrap();

        repo.save().await.unwrap();

        (device, token)
    }
}
