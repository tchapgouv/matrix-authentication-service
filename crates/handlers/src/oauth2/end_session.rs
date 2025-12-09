// Copyright 2024, 2025 New Vector Ltd.
// Copyright 2023, 2024 The Matrix.org Foundation C.I.C.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE files in the repository root for full details.
use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::Query;
use hyper::StatusCode;
use mas_axum_utils::{SessionInfoExt, cookies::CookieJar, record_error};
use mas_data_model::{BoxClock, BoxRng};
use mas_keystore::Keystore;
use mas_oidc_client::{
    requests::jose::{JwtVerificationData, verify_signed_jwt},
};
use mas_router::UrlBuilder;
use mas_storage::{
    BoxRepository, RepositoryAccess,
    queue::{QueueJobRepositoryExt as _, SyncDevicesJob},
    user::BrowserSessionRepository,
};
use oauth2_types::errors::{ClientError, ClientErrorCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{BoundActivityTracker, impl_from_error_for_route};

use mas_oidc_client::error::JwtVerificationError;
use tracing::info;

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct EndSessionParam {
    id_token_hint: String,
    post_logout_redirect_uri: String,
}

#[derive(Debug, Error)]
pub(crate) enum RouteError {
    #[error(transparent)]
    Internal(Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error("bad request")]
    BadRequest,

    #[error("client not found")]
    ClientNotFound,

    #[error("client is unauthorized")]
    UnauthorizedClient,

    // #[error("unsupported token type")]
    // UnsupportedTokenType,
    #[error("unknown token")]
    UnknownToken,
}

impl_from_error_for_route!(mas_storage::RepositoryError);

impl IntoResponse for RouteError {
    fn into_response(self) -> Response {
        let sentry_event_id = record_error!(self, Self::Internal(_));
        let response = match self {
            Self::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ClientError::from(ClientErrorCode::ServerError)),
            )
                .into_response(),

            Self::BadRequest => (
                StatusCode::BAD_REQUEST,
                Json(ClientError::from(ClientErrorCode::InvalidRequest)),
            )
                .into_response(),

            Self::ClientNotFound => (
                StatusCode::UNAUTHORIZED,
                Json(ClientError::from(ClientErrorCode::InvalidClient)),
            )
                .into_response(),

            // Self::ClientNotAllowed |
            Self::UnauthorizedClient => (
                StatusCode::UNAUTHORIZED,
                Json(ClientError::from(ClientErrorCode::UnauthorizedClient)),
            )
                .into_response(),

            // Self::UnsupportedTokenType => (
            //     StatusCode::BAD_REQUEST,
            //     Json(ClientError::from(ClientErrorCode::UnsupportedTokenType)),
            // )
            //     .into_response(),

            // If the token is unknown, we still return a 200 OK response.
            Self::UnknownToken => StatusCode::OK.into_response(),
        };

        (sentry_event_id, response).into_response()
    }
}

impl From<JwtVerificationError> for RouteError {
    fn from(_e: JwtVerificationError) -> Self {
        info!(%_e);
        Self::UnknownToken
    }
}

#[tracing::instrument(name = "handlers.oauth2.end_session.get", skip_all)]
pub(crate) async fn get(
    mut rng: BoxRng,
    clock: BoxClock,
    State(key_store): State<Keystore>,
    State(url_builder): State<UrlBuilder>,
    mut repo: BoxRepository,
    activity_tracker: BoundActivityTracker,
    Query(params): Query<EndSessionParam>,
    cookie_jar: CookieJar,
) -> Result<Response, RouteError> {
    let (session_info, cookie_jar) = cookie_jar.session_info();

    let browser_session_id = session_info
        .current_session_id()
        .ok_or(RouteError::BadRequest)?;

    let browser_session = repo
        .browser_session()
        .lookup(browser_session_id)
        .await?
        .ok_or(RouteError::BadRequest)?;

    info!(%browser_session.id);

    let oauth_session = repo
        .oauth2_session()
        .find_by_browser_session(browser_session.id)
        .await?
        .ok_or(RouteError::BadRequest)?;

    info!(%oauth_session.id);
    info!(%oauth_session.client_id);
    let client = repo
        .oauth2_client()
        .lookup(oauth_session.client_id)
        .await?
        .filter(|client| client.id_token_signed_response_alg.is_some())
        .ok_or(RouteError::ClientNotFound)?;

    let jwks = key_store.public_jwks();
    let issuer: String = url_builder.oidc_issuer().into();
    info!(%issuer);

    let id_token_verification_data = JwtVerificationData {
        issuer: Some(&issuer),
        jwks: &jwks,
        signing_algorithm: &client.id_token_signed_response_alg.unwrap(),
        client_id: &client.client_id,
    };

    info!("id_token_verification_data");
    verify_signed_jwt(
        &params.id_token_hint,
        id_token_verification_data,
    )?;

    // Check that the session is still valid.
    if !oauth_session.is_valid() {
        info!("NOT VALID : oauth_session.is_valid");
        // If the session is not valid, we redirect to post logout uri
        return Ok((cookie_jar, Redirect::to(&params.post_logout_redirect_uri)).into_response());
    }

    // Check that the client ending the session is the same as the client that
    // created it.
    if client.id != oauth_session.client_id {
        info!("NOT VALID : client.id != oauth_session.client_id");
        return Err(RouteError::UnauthorizedClient);
    }

    activity_tracker
        .record_oauth2_session(&clock, &oauth_session)
        .await;

    // If the session is associated with a user, make sure we schedule a device
    // deletion job for all the devices associated with the session.
    if let Some(user_id) = oauth_session.user_id {
        info!(%user_id);
        // Fetch the user
        let user = repo
            .user()
            .lookup(user_id)
            .await?
            .ok_or(RouteError::UnknownToken)?;

        // Schedule a job to sync the devices of the user with the homeserver
        repo.queue_job()
            .schedule_job(&mut rng, &clock, SyncDevicesJob::new(&user))
            .await?;
    }

    // Now that we checked everything, we can end the session.
    repo.oauth2_session().finish(&clock, oauth_session).await?;

    activity_tracker
        .record_browser_session(&clock, &browser_session)
        .await;
    repo.browser_session()
        .finish(&clock, browser_session)
        .await?;

    repo.save().await?;

    // We always want to clear out the session cookie, even if the session was
    // invalid
    let cookie_jar = cookie_jar.update_session_info(&session_info.mark_session_ended());

    Ok((cookie_jar, Redirect::to(&params.post_logout_redirect_uri)).into_response())
}


#[cfg(test)]
mod tests {
    use chrono::{Utc, Duration};
    use hyper::{Request, StatusCode};
    use mas_axum_utils::SessionInfoExt;
    use mas_data_model::Session;
    use mas_data_model::{Clock as _, Device};
    use sqlx::PgPool;
    use mas_router::SimpleRoute;
    use tracing::info;

    use crate::test_utils::{CookieHelper, RequestBuilderExt, ResponseExt, TestState, setup};

    use serde::Serialize;
    use mas_axum_utils::SessionInfo;
    use oauth2_types::scope::OPENID;
    use oauth2_types::scope::Scope;
    use oauth2_types::registration::ClientRegistrationResponse;
    use mas_iana::jose::JsonWebSignatureAlg;
    use mas_jose::jwt::{JsonWebSignatureHeader, Jwt};

    #[derive(Serialize)]
    struct Query {
        id_token_hint: String,
        post_logout_redirect_uri: String,
    }


    #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    async fn test_end_sessions(pool: PgPool) {

        setup();
        let state = TestState::from_pool(pool).await.unwrap();
        let mut rng = state.rng();

        // Provision a client
        let request =
            Request::post(mas_router::OAuth2RegistrationEndpoint::PATH).json(serde_json::json!({
                "client_uri": "https://example.com/",
                "redirect_uris": ["https://example.com/callback"],
                "token_endpoint_auth_method": "none",
                "response_types": ["code"],
                "grant_types": ["authorization_code", "refresh_token"],
                "id_token_signed_response_alg": "RS256",
            }));

        let response = state.request(request).await;
        response.assert_status(StatusCode::CREATED);

        let ClientRegistrationResponse { client_id, .. } = response.json();

        // Provision a provider and a link
        let mut repo = state.repository().await.unwrap();
        let user = repo
        .user()
        .add(&mut rng, &state.clock, "alice".to_owned())
        .await
        .unwrap();
        let browser_session = repo
                .browser_session()
                .add(&mut rng, &state.clock, &user, Some("Chrome".to_string()))
                .await
                .unwrap();
        
        // Lookup the client in the database.
        let client = repo
            .oauth2_client()
            .find_by_client_id(&client_id)
            .await
            .unwrap()
            .unwrap();
        info!(%client.id);
        let oauth2_session: Session = repo
        .oauth2_session()
        .add_from_browser_session(
            &mut state.rng(),
            &state.clock,
            &client,
            &browser_session,
            Scope::from_iter([OPENID]),
        )
        .await
        .unwrap();
        repo.save().await.unwrap();

        // Grab a key to sign the id_token
        // We could generate a key on the fly, but because we have one available here,
        // why not use it?
        let exp = Utc::now() +  Duration::minutes(10);
        let iat = Utc::now() -  Duration::minutes(10);
        let id_token_hint_claims = serde_json::json!({
            "sub": user.id,
            // "sid": sid,
            "aud": client_id,
            "exp": exp.timestamp(),
            "iat": iat.timestamp(),
            "iss": "https://example.com/",
        });

        // {
//   "iat": 1764689290,
//   "nonce": "5rODS4YNyb",
//   "c_hash": "-OmcZ7nYnONkQf4jKquFBQ",
//   "sub": "01HWCMZ39R7B7E3S7S9G6PXZNK",
//   "exp": 1764692890,
//   "aud": "01K6WGWE47QZXKB1M50DSHYN72",
//   "auth_time": 1764689288,
//   "at_hash": "76CITZVAFDkMItIwj6VCKQ",
//   "iss": "https://auth.dev01.tchap.incubateur.net/"
// }
        info!(%id_token_hint_claims);
        let key = state
            .key_store
            .signing_key_for_algorithm(&JsonWebSignatureAlg::Rs256)
            .unwrap();

        let signer = key
            .params()
            .signing_key_for_alg(&JsonWebSignatureAlg::Rs256)
            .unwrap();
        let header: JsonWebSignatureHeader = JsonWebSignatureHeader::new(JsonWebSignatureAlg::Rs256);
        let id_token_hint =
            Jwt::sign_with_rng(&mut rng, header, id_token_hint_claims.clone(), &signer).unwrap();

        let mut cookie_jar = state.cookie_jar();
        let info = SessionInfo::from_session(&browser_session);
        cookie_jar = cookie_jar.update_session_info(&info);
        let cookies = CookieHelper::new();
        cookies.import(cookie_jar);


        let q = Query {
            id_token_hint: id_token_hint.into_string(),
            post_logout_redirect_uri: "https://example.com/".to_string(),
        };
    
        let query = serde_urlencoded::to_string(q).unwrap();
        let url = format!("{}?{}", mas_router::OAuth2EndSession::PATH, query);
        info!("{}", url);
        let request = Request::get(url).empty();
        let request = cookies.with_cookies(request);
        let response = state.request(request).await;
        cookies.save_cookies(&response);
        response.assert_status(StatusCode::SEE_OTHER);
        
        // The finished_at timestamp should be the same as the current time
        let mut repo = state.repository().await.unwrap();
        let expected = repo
            .browser_session()
            .lookup(browser_session.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(expected.finished_at.unwrap(), state.clock.now());
        let expected_oauth2_session: Session = repo
            .oauth2_session()
            .lookup(oauth2_session.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(expected_oauth2_session.finished_at().unwrap(), state.clock.now());

    }

    #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    async fn test_end_sessions_when_no_browser_session(pool: PgPool) {

        setup();
        let state = TestState::from_pool(pool).await.unwrap();
        let mut rng = state.rng();

        // Provision a client
        let request =
            Request::post(mas_router::OAuth2RegistrationEndpoint::PATH).json(serde_json::json!({
                "client_uri": "https://example.com/",
                "redirect_uris": ["https://example.com/callback"],
                "token_endpoint_auth_method": "none",
                "response_types": ["code"],
                "grant_types": ["authorization_code", "refresh_token"],
                "id_token_signed_response_alg": "RS256",
            }));

        let response = state.request(request).await;
        response.assert_status(StatusCode::CREATED);

        let ClientRegistrationResponse { client_id, .. } = response.json();

        // Provision a provider and a link
        let mut repo = state.repository().await.unwrap();
        let user = repo
        .user()
        .add(&mut rng, &state.clock, "alice".to_owned())
        .await
        .unwrap();
        
        let browser_session = repo
                .compat_session()
                .add(&mut rng, &state.clock, &user, 
                    Device::from("AABBCCDDEE".to_owned()),
                    None,
                    false,
                    None)
                .await
                .unwrap();
        
        // Lookup the client in the database.
        let client = repo
            .oauth2_client()
            .find_by_client_id(&client_id)
            .await
            .unwrap()
            .unwrap();
        info!(%client.id);
        // let oauth2_session: Session = repo
        // .oauth2_session()
        // .add_from_browser_session(
        //     &mut state.rng(),
        //     &state.clock,
        //     &client,
        //     &browser_session,
        //     Scope::from_iter([OPENID]),
        // )
        // .await
        // .unwrap();
        repo.save().await.unwrap();

        // Grab a key to sign the id_token
        // We could generate a key on the fly, but because we have one available here,
        // why not use it?
        let exp = Utc::now() +  Duration::minutes(10);
        let iat = Utc::now() -  Duration::minutes(10);
        let id_token_hint_claims = serde_json::json!({
            "sub": user.id,
            // "sid": sid,
            "aud": client_id,
            "exp": exp.timestamp(),
            "iat": iat.timestamp(),
            "iss": "https://example.com/",
        });

        info!(%id_token_hint_claims);
        let key = state
            .key_store
            .signing_key_for_algorithm(&JsonWebSignatureAlg::Rs256)
            .unwrap();

        let signer = key
            .params()
            .signing_key_for_alg(&JsonWebSignatureAlg::Rs256)
            .unwrap();
        let header: JsonWebSignatureHeader = JsonWebSignatureHeader::new(JsonWebSignatureAlg::Rs256);
        let id_token_hint =
            Jwt::sign_with_rng(&mut rng, header, id_token_hint_claims.clone(), &signer).unwrap();

        let cookie_jar = state.cookie_jar();
        // cookie_jar = cookie_jar.session_info();
        let cookies = CookieHelper::new();
        cookies.import(cookie_jar);


        let q = Query {
            id_token_hint: id_token_hint.into_string(),
            post_logout_redirect_uri: "https://example.com/".to_string(),
        };
    
        let query = serde_urlencoded::to_string(q).unwrap();
        let url = format!("{}?{}", mas_router::OAuth2EndSession::PATH, query);
        info!("{}", url);
        let request = Request::get(url).empty();
        let request = cookies.with_cookies(request);
        let response = state.request(request).await;
        cookies.save_cookies(&response);
        response.assert_status(StatusCode::SEE_OTHER);
        
        // // The finished_at timestamp should be the same as the current time
        // let mut repo = state.repository().await.unwrap();
        // let expected = repo
        //     .browser_session()
        //     .lookup(browser_session.id)
        //     .await
        //     .unwrap()
        //     .unwrap();
        // assert_eq!(expected.finished_at.unwrap(), state.clock.now());
        // let expected_oauth2_session: Session = repo
        //     .oauth2_session()
        //     .lookup(oauth2_session.id)
        //     .await
        //     .unwrap()
        //     .unwrap();
        // assert_eq!(expected_oauth2_session.finished_at().unwrap(), state.clock.now());

    }

    // #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    // async fn test_end_sessions(pool: PgPool) {
    //     setup();
    //     let mut state = TestState::from_pool(pool).await.unwrap();
    //     let token = state.token_with_scope("urn:mas:admin").await;
    //     let mut rng = state.rng();

    //     // Provision a user and a compat session
    //     let mut repo = state.repository().await.unwrap();
    //     let user = repo
    //         .user()
    //         .add(&mut rng, &state.clock, "alice".to_owned())
    //         .await
    //         .unwrap();
    //     let device = Device::generate(&mut rng);
    //     let session = repo
    //         .compat_session()
    //         .add(&mut rng, &state.clock, &user, device, None, false, None)
    //         .await
    //         .unwrap();
    //     repo.save().await.unwrap();

    //     let request = Request::get(format!("/oauth2/end_session", &user.id))
    //         .bearer(&token)
    //         .empty();
    //     let response = state.request(request).await;
    //     response.assert_status(StatusCode::OK);
    //     let body: serde_json::Value = response.json();

    //     assert_eq!(body["data"]["id"], format!("{}", &user.id));
    //     // The finished_at timestamp should be the same as the current time
    //     let mut repo = state.repository().await.unwrap();
    //     let expected = repo
    //         .compat_session()
    //         .lookup(session.id)
    //         .await
    //         .unwrap()
    //         .unwrap();
    //     assert_eq!(expected.finished_at().unwrap(), state.clock.now());
    // }

    // #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    // async fn test_kill_already_finished_session(pool: PgPool) {
    //     setup();
    //     let mut state = TestState::from_pool(pool).await.unwrap();
    //     let token = state.token_with_scope("urn:mas:admin").await;
    //     let mut rng = state.rng();

    //     // Provision a user and a compat session
    //     let mut repo = state.repository().await.unwrap();
    //     let user = repo
    //         .user()
    //         .add(&mut rng, &state.clock, "alice".to_owned())
    //         .await
    //         .unwrap();
    //     let device = Device::generate(&mut rng);
    //     let session = repo
    //         .compat_session()
    //         .add(&mut rng, &state.clock, &user, device, None, false, None)
    //         .await
    //         .unwrap();

    //     // Finish the session first
    //     let session = repo
    //         .compat_session()
    //         .finish(&state.clock, session)
    //         .await
    //         .unwrap();

    //     repo.save().await.unwrap();

    //     // Move the clock forward
    //     state.clock.advance(Duration::try_minutes(1).unwrap());

    //     let request = Request::post(format!("/api/admin/v1/users/{}/kill-sessions", &user.id))
    //         .bearer(&token)
    //         .empty();
    //     let response = state.request(request).await;
    //     response.assert_status(StatusCode::OK);
    //     let body: serde_json::Value = response.json();

    //     assert_eq!(body["data"]["id"], format!("{}", &user.id));
    //     let mut repo = state.repository().await.unwrap();
    //     let expected = repo
    //         .compat_session()
    //         .lookup(session.id)
    //         .await
    //         .unwrap()
    //         .unwrap();
    //     assert_ne!(expected.finished_at().unwrap(), state.clock.now());
    // }

    // #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    // async fn test_kill_sessions_on_unknown_users(pool: PgPool) {
    //     setup();
    //     let mut state = TestState::from_pool(pool).await.unwrap();
    //     let token = state.token_with_scope("urn:mas:admin").await;

    //     let request = Request::post("/api/admin/v1/users/01040G2081040G2081040G2081/kill-sessions")
    //         .bearer(&token)
    //         .empty();
    //     let response = state.request(request).await;
    //     response.assert_status(StatusCode::NOT_FOUND);
    // }
}