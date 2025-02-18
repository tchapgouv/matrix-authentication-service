// Copyright 2024 New Vector Ltd.
// Copyright 2023, 2024 The Matrix.org Foundation C.I.C.
//
// SPDX-License-Identifier: AGPL-3.0-only
// Please see LICENSE in the repository root for full details.

use anyhow::Context;
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Response},
    Form,
};
use axum_extra::TypedHeader;
use mas_axum_utils::{
    cookies::CookieJar,
    csrf::{CsrfExt, ProtectedForm},
    FancyError, SessionInfoExt,
};
use mas_policy::Policy;
use mas_router::UrlBuilder;
use mas_storage::{BoxClock, BoxRepository, BoxRng};
use mas_templates::{DeviceConsentContext, PolicyViolationContext, TemplateContext, Templates};
use serde::Deserialize;
use tracing::warn;
use ulid::Ulid;

use crate::{BoundActivityTracker, PreferredLanguage};

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
enum Action {
    Consent,
    Reject,
}

#[derive(Deserialize, Debug)]
pub(crate) struct ConsentForm {
    action: Action,
}

pub(crate) async fn get(
    mut rng: BoxRng,
    clock: BoxClock,
    PreferredLanguage(locale): PreferredLanguage,
    State(templates): State<Templates>,
    State(url_builder): State<UrlBuilder>,
    mut repo: BoxRepository,
    mut policy: Policy,
    activity_tracker: BoundActivityTracker,
    user_agent: Option<TypedHeader<headers::UserAgent>>,
    cookie_jar: CookieJar,
    Path(grant_id): Path<Ulid>,
) -> Result<Response, FancyError> {
    let (session_info, cookie_jar) = cookie_jar.session_info();
    let (csrf_token, cookie_jar) = cookie_jar.csrf_token(&clock, &mut rng);

    let maybe_session = session_info.load_session(&mut repo).await?;

    let user_agent = user_agent.map(|ua| ua.to_string());

    let Some(session) = maybe_session else {
        let login = mas_router::Login::and_continue_device_code_grant(grant_id);
        return Ok((cookie_jar, url_builder.redirect(&login)).into_response());
    };

    activity_tracker
        .record_browser_session(&clock, &session)
        .await;

    // TODO: better error handling
    let grant = repo
        .oauth2_device_code_grant()
        .lookup(grant_id)
        .await?
        .context("Device grant not found")?;

    if grant.expires_at < clock.now() {
        return Err(FancyError::from(anyhow::anyhow!("Grant is expired")));
    }

    let client = repo
        .oauth2_client()
        .lookup(grant.client_id)
        .await?
        .context("Client not found")?;

    // Evaluate the policy
    let res = policy
        .evaluate_authorization_grant(mas_policy::AuthorizationGrantInput {
            grant_type: mas_policy::GrantType::DeviceCode,
            client: &client,
            scope: &grant.scope,
            user: Some(&session.user),
            requester: mas_policy::Requester {
                ip_address: activity_tracker.ip(),
                user_agent,
            },
        })
        .await?;
    if !res.valid() {
        warn!(violation = ?res, "Device code grant for client {} denied by policy", client.id);

        let (csrf_token, cookie_jar) = cookie_jar.csrf_token(&clock, &mut rng);
        let ctx = PolicyViolationContext::for_device_code_grant(grant, client)
            .with_session(session)
            .with_csrf(csrf_token.form_value())
            .with_language(locale);

        let content = templates.render_policy_violation(&ctx)?;

        return Ok((cookie_jar, Html(content)).into_response());
    }

    let ctx = DeviceConsentContext::new(grant, client)
        .with_session(session)
        .with_csrf(csrf_token.form_value())
        .with_language(locale);

    let rendered = templates
        .render_device_consent(&ctx)
        .context("Failed to render template")?;

    Ok((cookie_jar, Html(rendered)).into_response())
}

pub(crate) async fn post(
    mut rng: BoxRng,
    clock: BoxClock,
    PreferredLanguage(locale): PreferredLanguage,
    State(templates): State<Templates>,
    State(url_builder): State<UrlBuilder>,
    mut repo: BoxRepository,
    mut policy: Policy,
    activity_tracker: BoundActivityTracker,
    user_agent: Option<TypedHeader<headers::UserAgent>>,
    cookie_jar: CookieJar,
    Path(grant_id): Path<Ulid>,
    Form(form): Form<ProtectedForm<ConsentForm>>,
) -> Result<Response, FancyError> {
    let (session_info, cookie_jar) = cookie_jar.session_info();
    let form = cookie_jar.verify_form(&clock, form)?;
    let (csrf_token, cookie_jar) = cookie_jar.csrf_token(&clock, &mut rng);

    let maybe_session = session_info.load_session(&mut repo).await?;

    let user_agent = user_agent.map(|TypedHeader(ua)| ua.to_string());

    let Some(session) = maybe_session else {
        let login = mas_router::Login::and_continue_device_code_grant(grant_id);
        return Ok((cookie_jar, url_builder.redirect(&login)).into_response());
    };

    activity_tracker
        .record_browser_session(&clock, &session)
        .await;

    // TODO: better error handling
    let grant = repo
        .oauth2_device_code_grant()
        .lookup(grant_id)
        .await?
        .context("Device grant not found")?;

    if grant.expires_at < clock.now() {
        return Err(FancyError::from(anyhow::anyhow!("Grant is expired")));
    }

    let client = repo
        .oauth2_client()
        .lookup(grant.client_id)
        .await?
        .context("Client not found")?;

    // Evaluate the policy
    let res = policy
        .evaluate_authorization_grant(mas_policy::AuthorizationGrantInput {
            grant_type: mas_policy::GrantType::DeviceCode,
            client: &client,
            scope: &grant.scope,
            user: Some(&session.user),
            requester: mas_policy::Requester {
                ip_address: activity_tracker.ip(),
                user_agent,
            },
        })
        .await?;
    if !res.valid() {
        warn!(violation = ?res, "Device code grant for client {} denied by policy", client.id);

        let (csrf_token, cookie_jar) = cookie_jar.csrf_token(&clock, &mut rng);
        let ctx = PolicyViolationContext::for_device_code_grant(grant, client)
            .with_session(session)
            .with_csrf(csrf_token.form_value())
            .with_language(locale);

        let content = templates.render_policy_violation(&ctx)?;

        return Ok((cookie_jar, Html(content)).into_response());
    }

    let grant = if grant.is_pending() {
        match form.action {
            Action::Consent => {
                repo.oauth2_device_code_grant()
                    .fulfill(&clock, grant, &session)
                    .await?
            }
            Action::Reject => {
                repo.oauth2_device_code_grant()
                    .reject(&clock, grant, &session)
                    .await?
            }
        }
    } else {
        // XXX: In case we're not pending, let's just return the grant as-is
        // since it might just be a form resubmission, and feedback is nice enough
        warn!(
            oauth2_device_code.id = %grant.id,
            browser_session.id = %session.id,
            user.id = %session.user.id,
            "Grant is not pending",
        );
        grant
    };

    repo.save().await?;

    let ctx = DeviceConsentContext::new(grant, client)
        .with_session(session)
        .with_csrf(csrf_token.form_value())
        .with_language(locale);

    let rendered = templates
        .render_device_consent(&ctx)
        .context("Failed to render template")?;

    Ok((cookie_jar, Html(rendered)).into_response())
}
