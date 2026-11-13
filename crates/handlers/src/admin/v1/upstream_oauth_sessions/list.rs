// Copyright 2026 New Vector Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE files in the repository root for full details.

use aide::{OperationIo, transform::TransformOperation};
use axum::{Json, response::IntoResponse};
use axum_extra::extract::{Query, QueryRejection};
use axum_macros::FromRequestParts;
use hyper::StatusCode;
use mas_axum_utils::record_error;
use mas_storage::{Page, upstream_oauth2::UpstreamOAuthSessionFilter};
use schemars::JsonSchema;
use serde::Deserialize;
use ulid::Ulid;

use crate::{
    admin::{
        call_context::CallContext,
        model::{Resource, UpstreamOAuthSession},
        params::{IncludeCount, Pagination},
        response::{ErrorResponse, PaginatedResponse},
    },
    impl_from_error_for_route,
};

#[derive(FromRequestParts, Deserialize, JsonSchema, OperationIo)]
#[serde(rename = "UpstreamOAuthSessionFilter")]
#[aide(input_with = "Query<FilterParams>")]
#[from_request(via(Query), rejection(RouteError))]
pub struct FilterParams {
    /// Retrieve the items for the given provider
    #[serde(rename = "filter[provider]")]
    #[schemars(with = "Option<crate::admin::schema::Ulid>")]
    provider: Option<Ulid>,

    /// Retrieve the items with the given `sub` claim in the ID token
    #[serde(rename = "filter[sub-claim]")]
    sub_claim: Option<String>,

    /// Retrieve the items with the given `sid` claim in the ID token
    #[serde(rename = "filter[sid-claim]")]
    sid_claim: Option<String>,
}

impl std::fmt::Display for FilterParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut sep = '?';

        if let Some(provider) = self.provider {
            write!(f, "{sep}filter[provider]={provider}")?;
            sep = '&';
        }

        if let Some(sub_claim) = &self.sub_claim {
            write!(f, "{sep}filter[sub-claim]={sub_claim}")?;
            sep = '&';
        }

        if let Some(sid_claim) = &self.sid_claim {
            write!(f, "{sep}filter[sid-claim]={sid_claim}")?;
            sep = '&';
        }

        let _ = sep;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, OperationIo)]
#[aide(output_with = "Json<ErrorResponse>")]
pub enum RouteError {
    #[error(transparent)]
    Internal(Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error("Provider ID {0} not found")]
    ProviderNotFound(Ulid),

    #[error("Invalid filter parameters")]
    InvalidFilter(#[from] QueryRejection),
}

impl_from_error_for_route!(mas_storage::RepositoryError);

impl IntoResponse for RouteError {
    fn into_response(self) -> axum::response::Response {
        let error = ErrorResponse::from_error(&self);
        let sentry_event_id = record_error!(self, Self::Internal(_));
        let status = match self {
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ProviderNotFound(_) => StatusCode::NOT_FOUND,
            Self::InvalidFilter(_) => StatusCode::BAD_REQUEST,
        };
        (status, sentry_event_id, Json(error)).into_response()
    }
}

pub fn doc(operation: TransformOperation) -> TransformOperation {
    operation
        .id("listUpstreamOAuthSessions")
        .summary("List upstream OAuth 2.0 sessions")
        .description("Retrieve a list of upstream OAuth 2.0 sessions.")
        .tag("upstream-oauth-session")
        .response_with::<200, Json<PaginatedResponse<UpstreamOAuthSession>>, _>(|t| {
            let sessions = UpstreamOAuthSession::samples();
            let pagination = mas_storage::Pagination::first(sessions.len());
            let page = Page {
                edges: sessions
                    .into_iter()
                    .map(|node| mas_storage::pagination::Edge {
                        cursor: node.id(),
                        node,
                    })
                    .collect(),
                has_next_page: true,
                has_previous_page: false,
            };

            t.description("Paginated response of upstream OAuth 2.0 sessions")
                .example(PaginatedResponse::for_page(
                    page,
                    pagination,
                    Some(42),
                    UpstreamOAuthSession::PATH,
                ))
        })
        .response_with::<404, RouteError, _>(|t| {
            let response = ErrorResponse::from_error(&RouteError::ProviderNotFound(Ulid::nil()));
            t.description("Provider was not found").example(response)
        })
}

#[tracing::instrument(name = "handler.admin.v1.upstream_oauth_sessions.list", skip_all)]
pub async fn handler(
    CallContext { mut repo, .. }: CallContext,
    Pagination(pagination, include_count): Pagination,
    params: FilterParams,
) -> Result<Json<PaginatedResponse<UpstreamOAuthSession>>, RouteError> {
    let base = format!("{path}{params}", path = UpstreamOAuthSession::PATH);
    let base = include_count.add_to_base(&base);
    let filter = UpstreamOAuthSessionFilter::default();

    // Load the provider from the filter
    let maybe_provider = if let Some(provider_id) = params.provider {
        let provider = repo
            .upstream_oauth_provider()
            .lookup(provider_id)
            .await?
            .ok_or(RouteError::ProviderNotFound(provider_id))?;
        Some(provider)
    } else {
        None
    };

    let filter = if let Some(provider) = &maybe_provider {
        filter.for_provider(provider)
    } else {
        filter
    };

    let filter = if let Some(sub_claim) = &params.sub_claim {
        filter.with_sub_claim(sub_claim)
    } else {
        filter
    };

    let filter = if let Some(sid_claim) = &params.sid_claim {
        filter.with_sid_claim(sid_claim)
    } else {
        filter
    };

    let response = match include_count {
        IncludeCount::True => {
            let page = repo
                .upstream_oauth_session()
                .list(filter, pagination)
                .await?
                .map(UpstreamOAuthSession::from);
            let count = repo.upstream_oauth_session().count(filter).await?;
            PaginatedResponse::for_page(page, pagination, Some(count), &base)
        }
        IncludeCount::False => {
            let page = repo
                .upstream_oauth_session()
                .list(filter, pagination)
                .await?
                .map(UpstreamOAuthSession::from);
            PaginatedResponse::for_page(page, pagination, None, &base)
        }
        IncludeCount::Only => {
            let count = repo.upstream_oauth_session().count(filter).await?;
            PaginatedResponse::for_count_only(count, &base)
        }
    };

    Ok(Json(response))
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
    async fn test_list(pool: PgPool) {
        setup();
        let mut state = TestState::from_pool(pool).await.unwrap();
        let token = state.token_with_scope("urn:mas:admin").await;
        let mut rng = state.rng();

        // Provision providers, links and sessions
        let mut repo = state.repository().await.unwrap();
        let provider1 = repo
            .upstream_oauth_provider()
            .add(
                &mut rng,
                &state.clock,
                test_utils::oidc_provider_params("acme"),
            )
            .await
            .unwrap();
        let provider2 = repo
            .upstream_oauth_provider()
            .add(
                &mut rng,
                &state.clock,
                test_utils::oidc_provider_params("example"),
            )
            .await
            .unwrap();

        let link1 = repo
            .upstream_oauth_link()
            .add(
                &mut rng,
                &state.clock,
                &provider1,
                "subject1".to_owned(),
                None,
            )
            .await
            .unwrap();
        let link2 = repo
            .upstream_oauth_link()
            .add(
                &mut rng,
                &state.clock,
                &provider2,
                "subject2".to_owned(),
                None,
            )
            .await
            .unwrap();

        let session1 = repo
            .upstream_oauth_session()
            .add(
                &mut rng,
                &state.clock,
                &provider1,
                "state1".to_owned(),
                None,
                None,
            )
            .await
            .unwrap();
        let session1 = repo
            .upstream_oauth_session()
            .complete_with_link(
                &state.clock,
                session1,
                &link1,
                Some("header.payload.signature".to_owned()),
                Some(serde_json::json!({
                    "sub": "subject1",
                    "email": "alice@acme.example.com",
                    "email_verified": true
                })),
                None,
                None,
            )
            .await
            .unwrap();

        let session2 = repo
            .upstream_oauth_session()
            .add(
                &mut rng,
                &state.clock,
                &provider1,
                "state2".to_owned(),
                None,
                None,
            )
            .await
            .unwrap();

        let session3 = repo
            .upstream_oauth_session()
            .add(
                &mut rng,
                &state.clock,
                &provider2,
                "state3".to_owned(),
                None,
                None,
            )
            .await
            .unwrap();
        let session3 = repo
            .upstream_oauth_session()
            .complete_with_link(
                &state.clock,
                session3,
                &link2,
                None,
                Some(serde_json::json!({
                    "sub": "subject2",
                    "email": "bob@example.example.com",
                    "email_verified": false
                })),
                None,
                Some(serde_json::json!({
                    "sub": "subject2",
                    "name": "Bob"
                })),
            )
            .await
            .unwrap();

        repo.save().await.unwrap();

        // List all sessions
        let request = Request::get("/api/admin/v1/upstream-oauth-sessions")
            .bearer(&token)
            .empty();
        let response = state.request(request).await;
        response.assert_status(StatusCode::OK);
        let body: serde_json::Value = response.json();
        assert_json_snapshot!(body, @r#"
        {
          "meta": {
            "count": 3
          },
          "data": [
            {
              "type": "upstream-oauth-session",
              "id": "01FSHN9AG0AQZQP8DX40GD59PW",
              "attributes": {
                "created_at": "2022-01-16T14:40:00Z",
                "provider_id": "01FSHN9AG0MZAA6S4AF7CTV32E",
                "link_id": "01FSHN9AG09NMZYX8MFYH578R9",
                "completed_at": "2022-01-16T14:40:00Z",
                "consumed_at": null,
                "unlinked_at": null,
                "id_token_claims": {
                  "sub": "subject1",
                  "email": "alice@acme.example.com",
                  "email_verified": true
                },
                "userinfo": null,
                "extra_callback_parameters": null
              },
              "links": {
                "self": "/api/admin/v1/upstream-oauth-sessions/01FSHN9AG0AQZQP8DX40GD59PW"
              },
              "meta": {
                "page": {
                  "cursor": "01FSHN9AG0AQZQP8DX40GD59PW"
                }
              }
            },
            {
              "type": "upstream-oauth-session",
              "id": "01FSHN9AG0PJZ6DZNTAA1XKPT4",
              "attributes": {
                "created_at": "2022-01-16T14:40:00Z",
                "provider_id": "01FSHN9AG0AJ6AC5HQ9X6H4RP4",
                "link_id": "01FSHN9AG0KEPHYQQXW9XPTX6Z",
                "completed_at": "2022-01-16T14:40:00Z",
                "consumed_at": null,
                "unlinked_at": null,
                "id_token_claims": {
                  "sub": "subject2",
                  "email": "bob@example.example.com",
                  "email_verified": false
                },
                "userinfo": {
                  "sub": "subject2",
                  "name": "Bob"
                },
                "extra_callback_parameters": null
              },
              "links": {
                "self": "/api/admin/v1/upstream-oauth-sessions/01FSHN9AG0PJZ6DZNTAA1XKPT4"
              },
              "meta": {
                "page": {
                  "cursor": "01FSHN9AG0PJZ6DZNTAA1XKPT4"
                }
              }
            },
            {
              "type": "upstream-oauth-session",
              "id": "01FSHN9AG0QHEHKX2JNQ2A2D07",
              "attributes": {
                "created_at": "2022-01-16T14:40:00Z",
                "provider_id": "01FSHN9AG0MZAA6S4AF7CTV32E",
                "link_id": null,
                "completed_at": null,
                "consumed_at": null,
                "unlinked_at": null,
                "id_token_claims": null,
                "userinfo": null,
                "extra_callback_parameters": null
              },
              "links": {
                "self": "/api/admin/v1/upstream-oauth-sessions/01FSHN9AG0QHEHKX2JNQ2A2D07"
              },
              "meta": {
                "page": {
                  "cursor": "01FSHN9AG0QHEHKX2JNQ2A2D07"
                }
              }
            }
          ],
          "links": {
            "self": "/api/admin/v1/upstream-oauth-sessions?page[first]=10",
            "first": "/api/admin/v1/upstream-oauth-sessions?page[first]=10",
            "last": "/api/admin/v1/upstream-oauth-sessions?page[last]=10"
          }
        }
        "#);

        // Filter by provider
        let request = Request::get(format!(
            "/api/admin/v1/upstream-oauth-sessions?filter[provider]={}",
            provider1.id
        ))
        .bearer(&token)
        .empty();
        let response = state.request(request).await;
        response.assert_status(StatusCode::OK);
        let body: serde_json::Value = response.json();
        assert_eq!(body["meta"]["count"], 2);
        assert_json_snapshot!(body, @r#"
        {
          "meta": {
            "count": 2
          },
          "data": [
            {
              "type": "upstream-oauth-session",
              "id": "01FSHN9AG0AQZQP8DX40GD59PW",
              "attributes": {
                "created_at": "2022-01-16T14:40:00Z",
                "provider_id": "01FSHN9AG0MZAA6S4AF7CTV32E",
                "link_id": "01FSHN9AG09NMZYX8MFYH578R9",
                "completed_at": "2022-01-16T14:40:00Z",
                "consumed_at": null,
                "unlinked_at": null,
                "id_token_claims": {
                  "sub": "subject1",
                  "email": "alice@acme.example.com",
                  "email_verified": true
                },
                "userinfo": null,
                "extra_callback_parameters": null
              },
              "links": {
                "self": "/api/admin/v1/upstream-oauth-sessions/01FSHN9AG0AQZQP8DX40GD59PW"
              },
              "meta": {
                "page": {
                  "cursor": "01FSHN9AG0AQZQP8DX40GD59PW"
                }
              }
            },
            {
              "type": "upstream-oauth-session",
              "id": "01FSHN9AG0QHEHKX2JNQ2A2D07",
              "attributes": {
                "created_at": "2022-01-16T14:40:00Z",
                "provider_id": "01FSHN9AG0MZAA6S4AF7CTV32E",
                "link_id": null,
                "completed_at": null,
                "consumed_at": null,
                "unlinked_at": null,
                "id_token_claims": null,
                "userinfo": null,
                "extra_callback_parameters": null
              },
              "links": {
                "self": "/api/admin/v1/upstream-oauth-sessions/01FSHN9AG0QHEHKX2JNQ2A2D07"
              },
              "meta": {
                "page": {
                  "cursor": "01FSHN9AG0QHEHKX2JNQ2A2D07"
                }
              }
            }
          ],
          "links": {
            "self": "/api/admin/v1/upstream-oauth-sessions?filter[provider]=01FSHN9AG0MZAA6S4AF7CTV32E&page[first]=10",
            "first": "/api/admin/v1/upstream-oauth-sessions?filter[provider]=01FSHN9AG0MZAA6S4AF7CTV32E&page[first]=10",
            "last": "/api/admin/v1/upstream-oauth-sessions?filter[provider]=01FSHN9AG0MZAA6S4AF7CTV32E&page[last]=10"
          }
        }
        "#);

        // Filter by sub claim
        let request =
            Request::get("/api/admin/v1/upstream-oauth-sessions?filter[sub-claim]=subject2")
                .bearer(&token)
                .empty();
        let response = state.request(request).await;
        response.assert_status(StatusCode::OK);
        let body: serde_json::Value = response.json();
        assert_eq!(body["meta"]["count"], 1);
        assert_json_snapshot!(body, @r#"
        {
          "meta": {
            "count": 1
          },
          "data": [
            {
              "type": "upstream-oauth-session",
              "id": "01FSHN9AG0PJZ6DZNTAA1XKPT4",
              "attributes": {
                "created_at": "2022-01-16T14:40:00Z",
                "provider_id": "01FSHN9AG0AJ6AC5HQ9X6H4RP4",
                "link_id": "01FSHN9AG0KEPHYQQXW9XPTX6Z",
                "completed_at": "2022-01-16T14:40:00Z",
                "consumed_at": null,
                "unlinked_at": null,
                "id_token_claims": {
                  "sub": "subject2",
                  "email": "bob@example.example.com",
                  "email_verified": false
                },
                "userinfo": {
                  "sub": "subject2",
                  "name": "Bob"
                },
                "extra_callback_parameters": null
              },
              "links": {
                "self": "/api/admin/v1/upstream-oauth-sessions/01FSHN9AG0PJZ6DZNTAA1XKPT4"
              },
              "meta": {
                "page": {
                  "cursor": "01FSHN9AG0PJZ6DZNTAA1XKPT4"
                }
              }
            }
          ],
          "links": {
            "self": "/api/admin/v1/upstream-oauth-sessions?filter[sub-claim]=subject2&page[first]=10",
            "first": "/api/admin/v1/upstream-oauth-sessions?filter[sub-claim]=subject2&page[first]=10",
            "last": "/api/admin/v1/upstream-oauth-sessions?filter[sub-claim]=subject2&page[last]=10"
          }
        }
        "#);

        // Get the most recent session with backward pagination
        let most_recent_id = [session1.id, session2.id, session3.id]
            .into_iter()
            .max()
            .unwrap();
        let request =
            Request::get("/api/admin/v1/upstream-oauth-sessions?page[last]=1&count=false")
                .bearer(&token)
                .empty();
        let response = state.request(request).await;
        response.assert_status(StatusCode::OK);
        let body: serde_json::Value = response.json();
        assert_eq!(body["data"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"][0]["id"], most_recent_id.to_string());
        assert_json_snapshot!(body, @r#"
        {
          "data": [
            {
              "type": "upstream-oauth-session",
              "id": "01FSHN9AG0QHEHKX2JNQ2A2D07",
              "attributes": {
                "created_at": "2022-01-16T14:40:00Z",
                "provider_id": "01FSHN9AG0MZAA6S4AF7CTV32E",
                "link_id": null,
                "completed_at": null,
                "consumed_at": null,
                "unlinked_at": null,
                "id_token_claims": null,
                "userinfo": null,
                "extra_callback_parameters": null
              },
              "links": {
                "self": "/api/admin/v1/upstream-oauth-sessions/01FSHN9AG0QHEHKX2JNQ2A2D07"
              },
              "meta": {
                "page": {
                  "cursor": "01FSHN9AG0QHEHKX2JNQ2A2D07"
                }
              }
            }
          ],
          "links": {
            "self": "/api/admin/v1/upstream-oauth-sessions?count=false&page[last]=1",
            "first": "/api/admin/v1/upstream-oauth-sessions?count=false&page[first]=1",
            "last": "/api/admin/v1/upstream-oauth-sessions?count=false&page[last]=1",
            "prev": "/api/admin/v1/upstream-oauth-sessions?count=false&page[before]=01FSHN9AG0QHEHKX2JNQ2A2D07&page[last]=1"
          }
        }
        "#);

        // Test count=only
        let request = Request::get("/api/admin/v1/upstream-oauth-sessions?count=only")
            .bearer(&token)
            .empty();
        let response = state.request(request).await;
        response.assert_status(StatusCode::OK);
        let body: serde_json::Value = response.json();
        assert_eq!(body["meta"]["count"], 3);
        assert_json_snapshot!(body, @r###"
        {
          "meta": {
            "count": 3
          },
          "links": {
            "self": "/api/admin/v1/upstream-oauth-sessions?count=only"
          }
        }
        "###);
    }

    #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    async fn test_provider_not_found(pool: PgPool) {
        setup();
        let mut state = TestState::from_pool(pool).await.unwrap();
        let token = state.token_with_scope("urn:mas:admin").await;

        let provider_id = Ulid::nil();
        let request = Request::get(format!(
            "/api/admin/v1/upstream-oauth-sessions?filter[provider]={provider_id}"
        ))
        .bearer(&token)
        .empty();

        let response = state.request(request).await;
        response.assert_status(StatusCode::NOT_FOUND);
    }
}
