// Copyright 2024 New Vector Ltd.
// Copyright 2024 The Matrix.org Foundation C.I.C.
//
// SPDX-License-Identifier: AGPL-3.0-only
// Please see LICENSE in the repository root for full details.

use std::sync::Arc;

use aide::{
    axum::ApiRouter,
    openapi::{OAuth2Flow, OAuth2Flows, OpenApi, SecurityScheme, Server, Tag},
    transform::TransformOpenApi,
};
use axum::{
    Json, Router,
    extract::{FromRef, FromRequestParts, State},
    http::HeaderName,
    response::Html,
};
use hyper::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use indexmap::IndexMap;
use mas_axum_utils::InternalError;
use mas_http::CorsLayerExt;
use mas_matrix::HomeserverConnection;
use mas_policy::PolicyFactory;
use mas_router::{
    ApiDoc, ApiDocCallback, OAuth2AuthorizationEndpoint, OAuth2TokenEndpoint, Route, SimpleRoute,
    UrlBuilder,
};
use mas_storage::BoxRng;
use mas_templates::{ApiDocContext, Templates};
use tower_http::cors::{Any, CorsLayer};

mod call_context;
mod model;
mod params;
mod response;
mod schema;
mod v1;

use self::call_context::CallContext;
use crate::passwords::PasswordManager;

fn finish(t: TransformOpenApi) -> TransformOpenApi {
    t.title("Matrix Authentication Service admin API")
        .tag(Tag {
            name: "compat-session".to_owned(),
            description: Some("Manage compatibility sessions from legacy clients".to_owned()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "policy-data".to_owned(),
            description: Some("Manage the dynamic policy data".to_owned()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "oauth2-session".to_owned(),
            description: Some("Manage OAuth2 sessions".to_owned()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "user".to_owned(),
            description: Some("Manage users".to_owned()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "user-email".to_owned(),
            description: Some("Manage emails associated with users".to_owned()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "user-session".to_owned(),
            description: Some("Manage browser sessions of users".to_owned()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "upstream-oauth-link".to_owned(),
            description: Some(
                "Manage links between local users and identities from upstream OAuth 2.0 providers"
                    .to_owned(),
            ),
            ..Default::default()
        })
        .security_scheme(
            "oauth2",
            SecurityScheme::OAuth2 {
                flows: OAuth2Flows {
                    client_credentials: Some(OAuth2Flow::ClientCredentials {
                        refresh_url: Some(OAuth2TokenEndpoint::PATH.to_owned()),
                        token_url: OAuth2TokenEndpoint::PATH.to_owned(),
                        scopes: IndexMap::from([(
                            "urn:mas:admin".to_owned(),
                            "Grant access to the admin API".to_owned(),
                        )]),
                    }),
                    authorization_code: Some(OAuth2Flow::AuthorizationCode {
                        authorization_url: OAuth2AuthorizationEndpoint::PATH.to_owned(),
                        refresh_url: Some(OAuth2TokenEndpoint::PATH.to_owned()),
                        token_url: OAuth2TokenEndpoint::PATH.to_owned(),
                        scopes: IndexMap::from([(
                            "urn:mas:admin".to_owned(),
                            "Grant access to the admin API".to_owned(),
                        )]),
                    }),
                    implicit: None,
                    password: None,
                },
                description: None,
                extensions: IndexMap::default(),
            },
        )
        .security_requirement_scopes("oauth2", ["urn:mas:admin"])
}

pub fn router<S>() -> (OpenApi, Router<S>)
where
    S: Clone + Send + Sync + 'static,
    Arc<dyn HomeserverConnection>: FromRef<S>,
    PasswordManager: FromRef<S>,
    BoxRng: FromRequestParts<S>,
    CallContext: FromRequestParts<S>,
    Templates: FromRef<S>,
    UrlBuilder: FromRef<S>,
    Arc<PolicyFactory>: FromRef<S>,
{
    // We *always* want to explicitly set the possible responses, beacuse the
    // infered ones are not necessarily correct
    aide::generate::infer_responses(false);

    aide::generate::in_context(|ctx| {
        ctx.schema =
            schemars::r#gen::SchemaGenerator::new(schemars::r#gen::SchemaSettings::openapi3());
    });

    let mut api = OpenApi::default();
    let router = ApiRouter::<S>::new()
        .nest("/api/admin/v1", self::v1::router())
        .finish_api_with(&mut api, finish);

    let router = router
        // Serve the OpenAPI spec as JSON
        .route(
            "/api/spec.json",
            axum::routing::get({
                let api = api.clone();
                move |State(url_builder): State<UrlBuilder>| {
                    // Let's set the servers to the HTTP base URL
                    let mut api = api.clone();
                    api.servers = vec![Server {
                        url: url_builder.http_base().to_string(),
                        ..Server::default()
                    }];

                    std::future::ready(Json(api))
                }
            }),
        )
        // Serve the Swagger API reference
        .route(ApiDoc::route(), axum::routing::get(swagger))
        .route(
            ApiDocCallback::route(),
            axum::routing::get(swagger_callback),
        )
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_otel_headers([
                    AUTHORIZATION,
                    ACCEPT,
                    CONTENT_TYPE,
                    // Swagger will send this header, so we have to allow it to avoid CORS errors
                    HeaderName::from_static("x-requested-with"),
                ]),
        );

    (api, router)
}

async fn swagger(
    State(url_builder): State<UrlBuilder>,
    State(templates): State<Templates>,
) -> Result<Html<String>, InternalError> {
    let ctx = ApiDocContext::from_url_builder(&url_builder);
    let res = templates.render_swagger(&ctx)?;
    Ok(Html(res))
}

async fn swagger_callback(
    State(url_builder): State<UrlBuilder>,
    State(templates): State<Templates>,
) -> Result<Html<String>, InternalError> {
    let ctx = ApiDocContext::from_url_builder(&url_builder);
    let res = templates.render_swagger_callback(&ctx)?;
    Ok(Html(res))
}
