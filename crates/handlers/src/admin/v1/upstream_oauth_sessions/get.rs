// Copyright 2026 New Vector Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE files in the repository root for full details.

use aide::{OperationIo, transform::TransformOperation};
use axum::{Json, response::IntoResponse};
use hyper::StatusCode;
use mas_axum_utils::record_error;
use ulid::Ulid;

use crate::{
    admin::{
        call_context::CallContext,
        model::UpstreamOAuthSession,
        params::UlidPathParam,
        response::{ErrorResponse, SingleResponse},
    },
    impl_from_error_for_route,
};

#[derive(Debug, thiserror::Error, OperationIo)]
#[aide(output_with = "Json<ErrorResponse>")]
pub enum RouteError {
    #[error(transparent)]
    Internal(Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error("Upstream OAuth 2.0 session ID {0} not found")]
    NotFound(Ulid),
}

impl_from_error_for_route!(mas_storage::RepositoryError);

impl IntoResponse for RouteError {
    fn into_response(self) -> axum::response::Response {
        let error = ErrorResponse::from_error(&self);
        let sentry_event_id = record_error!(self, Self::Internal(_));
        let status = match self {
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
        };
        (status, sentry_event_id, Json(error)).into_response()
    }
}

pub fn doc(operation: TransformOperation) -> TransformOperation {
    operation
        .id("getUpstreamOAuthSession")
        .summary("Get an upstream OAuth 2.0 session")
        .tag("upstream-oauth-session")
        .response_with::<200, Json<SingleResponse<UpstreamOAuthSession>>, _>(|t| {
            let [sample, ..] = UpstreamOAuthSession::samples();
            let response = SingleResponse::new_canonical(sample);
            t.description("Upstream OAuth 2.0 session was found")
                .example(response)
        })
        .response_with::<404, RouteError, _>(|t| {
            let response = ErrorResponse::from_error(&RouteError::NotFound(Ulid::nil()));
            t.description("Upstream OAuth 2.0 session was not found")
                .example(response)
        })
}

#[tracing::instrument(name = "handler.admin.v1.upstream_oauth_sessions.get", skip_all)]
pub async fn handler(
    CallContext { mut repo, .. }: CallContext,
    id: UlidPathParam,
) -> Result<Json<SingleResponse<UpstreamOAuthSession>>, RouteError> {
    let session = repo
        .upstream_oauth_session()
        .lookup(*id)
        .await?
        .ok_or(RouteError::NotFound(*id))?;

    Ok(Json(SingleResponse::new_canonical(
        UpstreamOAuthSession::from(session),
    )))
}

#[cfg(test)]
mod tests {
    use hyper::{Request, StatusCode};
    use insta::assert_json_snapshot;
    use sqlx::PgPool;
    use ulid::Ulid;

    use super::super::super::upstream_oauth_links::test_utils;
    use crate::test_utils::{RequestBuilderExt, ResponseExt, TestState, setup};

    #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    async fn test_get(pool: PgPool) {
        setup();
        let mut state = TestState::from_pool(pool).await.unwrap();
        let token = state.token_with_scope("urn:mas:admin").await;
        let mut rng = state.rng();

        // Provision a provider, a link and a completed session
        let mut repo = state.repository().await.unwrap();
        let provider = repo
            .upstream_oauth_provider()
            .add(
                &mut rng,
                &state.clock,
                test_utils::oidc_provider_params("provider1"),
            )
            .await
            .unwrap();
        let link = repo
            .upstream_oauth_link()
            .add(
                &mut rng,
                &state.clock,
                &provider,
                "subject1".to_owned(),
                None,
            )
            .await
            .unwrap();
        let session = repo
            .upstream_oauth_session()
            .add(
                &mut rng,
                &state.clock,
                &provider,
                "state".to_owned(),
                None,
                None,
            )
            .await
            .unwrap();
        let session = repo
            .upstream_oauth_session()
            .complete_with_link(
                &state.clock,
                session,
                &link,
                Some("header.payload.signature".to_owned()),
                Some(serde_json::json!({
                    "sub": "subject1",
                    "email": "alice@example.com",
                    "email_verified": true
                })),
                None,
                None,
            )
            .await
            .unwrap();
        repo.save().await.unwrap();

        let request = Request::get(format!(
            "/api/admin/v1/upstream-oauth-sessions/{}",
            session.id
        ))
        .bearer(&token)
        .empty();

        let response = state.request(request).await;
        response.assert_status(StatusCode::OK);
        let body: serde_json::Value = response.json();

        assert_eq!(
            body["data"]["attributes"]["id_token_claims"]["email"],
            "alice@example.com"
        );

        assert_json_snapshot!(body, @r#"
        {
          "data": {
            "type": "upstream-oauth-session",
            "id": "01FSHN9AG09NMZYX8MFYH578R9",
            "attributes": {
              "created_at": "2022-01-16T14:40:00Z",
              "provider_id": "01FSHN9AG0MZAA6S4AF7CTV32E",
              "link_id": "01FSHN9AG0AJ6AC5HQ9X6H4RP4",
              "completed_at": "2022-01-16T14:40:00Z",
              "consumed_at": null,
              "unlinked_at": null,
              "id_token_claims": {
                "sub": "subject1",
                "email": "alice@example.com",
                "email_verified": true
              },
              "userinfo": null,
              "extra_callback_parameters": null
            },
            "links": {
              "self": "/api/admin/v1/upstream-oauth-sessions/01FSHN9AG09NMZYX8MFYH578R9"
            }
          },
          "links": {
            "self": "/api/admin/v1/upstream-oauth-sessions/01FSHN9AG09NMZYX8MFYH578R9"
          }
        }
        "#);
    }

    #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    async fn test_not_found(pool: PgPool) {
        setup();
        let mut state = TestState::from_pool(pool).await.unwrap();
        let token = state.token_with_scope("urn:mas:admin").await;

        let session_id = Ulid::nil();
        let request = Request::get(format!(
            "/api/admin/v1/upstream-oauth-sessions/{session_id}"
        ))
        .bearer(&token)
        .empty();

        let response = state.request(request).await;
        response.assert_status(StatusCode::NOT_FOUND);
    }
}
