// Copyright 2024, 2025 New Vector Ltd.
// Copyright 2024 The Matrix.org Foundation C.I.C.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE files in the repository root for full details.

use std::{str::FromStr, sync::Arc};

use axum::{
    Form,
    extract::State,
    response::{Html, IntoResponse, Response},
};
use axum_extra::typed_header::TypedHeader;
use lettre::Address;
use mas_axum_utils::{
    InternalError, SessionInfoExt,
    cookies::CookieJar,
    csrf::{CsrfExt, ProtectedForm},
};
use mas_data_model::{BoxClock, BoxRng, SiteConfig, TchapConfig};
//:tchap:
use mas_matrix::HomeserverConnection;
//:tchap: end
use mas_router::UrlBuilder;
use mas_storage::{
    BoxRepository,
    queue::{QueueJobRepositoryExt as _, SendAccountRecoveryEmailsJob},
};
use mas_templates::{
    EmptyContext, FieldError, FormError, FormState, RecoveryStartContext, RecoveryStartFormField,
    TemplateContext, Templates,
};
use serde::{Deserialize, Serialize};
use tchap::EmailAllowedResult;

use crate::{BoundActivityTracker, Limiter, PreferredLanguage, RequesterFingerprint};

#[derive(Deserialize, Serialize)]
pub(crate) struct StartRecoveryForm {
    email: String,
}

pub(crate) async fn get(
    mut rng: BoxRng,
    clock: BoxClock,
    mut repo: BoxRepository,
    State(site_config): State<SiteConfig>,
    State(templates): State<Templates>,
    State(url_builder): State<UrlBuilder>,
    PreferredLanguage(locale): PreferredLanguage,
    cookie_jar: CookieJar,
) -> Result<Response, InternalError> {
    if !site_config.account_recovery_allowed {
        let context = EmptyContext.with_language(locale);
        let rendered = templates.render_recovery_disabled(&context)?;
        return Ok((cookie_jar, Html(rendered)).into_response());
    }

    let (session_info, cookie_jar) = cookie_jar.session_info();
    let (csrf_token, cookie_jar) = cookie_jar.csrf_token(&clock, &mut rng);

    let maybe_session = session_info.load_active_session(&mut repo).await?;
    if maybe_session.is_some() {
        // TODO: redirect to continue whatever action was going on
        return Ok((cookie_jar, url_builder.redirect(&mas_router::Index)).into_response());
    }

    let context = RecoveryStartContext::new()
        .with_csrf(csrf_token.form_value())
        .with_language(locale);

    repo.save().await?;

    let rendered = templates.render_recovery_start(&context)?;

    Ok((cookie_jar, Html(rendered)).into_response())
}

pub(crate) async fn post(
    mut rng: BoxRng,
    clock: BoxClock,
    mut repo: BoxRepository,
    user_agent: TypedHeader<headers::UserAgent>,
    activity_tracker: BoundActivityTracker,
    State(site_config): State<SiteConfig>,
    State(templates): State<Templates>,
    State(url_builder): State<UrlBuilder>,
    (State(limiter), requester): (State<Limiter>, RequesterFingerprint),
    //:tchap: add homeserver and tchap_config to check the email server
    State(homeserver): State<Arc<dyn HomeserverConnection>>,
    State(tchap_config): State<TchapConfig>,
    //:tchap: end
    PreferredLanguage(locale): PreferredLanguage,
    cookie_jar: CookieJar,
    Form(form): Form<ProtectedForm<StartRecoveryForm>>,
) -> Result<impl IntoResponse, InternalError> {
    if !site_config.account_recovery_allowed {
        let context = EmptyContext.with_language(locale);
        let rendered = templates.render_recovery_disabled(&context)?;
        return Ok((cookie_jar, Html(rendered)).into_response());
    }

    let (session_info, cookie_jar) = cookie_jar.session_info();
    let (csrf_token, cookie_jar) = cookie_jar.csrf_token(&clock, &mut rng);

    let maybe_session = session_info.load_active_session(&mut repo).await?;
    if maybe_session.is_some() {
        // TODO: redirect to continue whatever action was going on
        return Ok((cookie_jar, url_builder.redirect(&mas_router::Index)).into_response());
    }

    let user_agent = user_agent.as_str().to_owned();
    let ip_address = activity_tracker.ip();

    let form = cookie_jar.verify_form(&clock, form)?;
    let mut form_state = FormState::from_form(&form);

    if Address::from_str(&form.email).is_err() {
        form_state =
            form_state.with_error_on_field(RecoveryStartFormField::Email, FieldError::Invalid);
    }

    //:tchap:
    // If the email format is valid, verify it is allowed on this server
    // before any further processing. If the email is mapped to a different
    // server, the password reset will fail anyway, so we abort early with
    // a clear error message on the email field.
    if form_state.is_valid() {
        let email_check =
            check_email_allowed(&form.email, homeserver.homeserver(), &tchap_config).await;

        if let Ok(EmailAllowedResult::WrongServer {
            wrong_server_name,
            correct_server_name,
        }) = &email_check
        {
            tracing::warn!(
                ":tchap: - recovery - email {} is mapped to server {}, but current server is {}",
                form.email,
                correct_server_name,
                wrong_server_name
            );
            form_state.add_error_on_field(
                RecoveryStartFormField::Email,
                FieldError::Policy {
                    code: None,
                    message: format!(
                        "Votre adresse mail {email} est associée au serveur:{correct_server_name} hors vous êtes sur le serveur:{wrong_server_name}", email=form.email
                    ),
                },
            );
        }
    }
    //:tchap: end

    if form_state.is_valid() {
        // Check the rate limit if we are about to process the form
        if let Err(e) = limiter.check_account_recovery(requester, &form.email) {
            tracing::warn!(error = &e as &dyn std::error::Error);
            form_state.add_error_on_form(FormError::RateLimitExceeded);
        }
    }

    if !form_state.is_valid() {
        repo.save().await?;
        let context = RecoveryStartContext::new()
            .with_form_state(form_state)
            .with_csrf(csrf_token.form_value())
            .with_language(locale);

        let rendered = templates.render_recovery_start(&context)?;

        return Ok((cookie_jar, Html(rendered)).into_response());
    }

    let session = repo
        .user_recovery()
        .add_session(
            &mut rng,
            &clock,
            form.email,
            user_agent,
            ip_address,
            locale.to_string(),
        )
        .await?;

    repo.queue_job()
        .schedule_job(
            &mut rng,
            &clock,
            SendAccountRecoveryEmailsJob::new(&session),
        )
        .await?;

    repo.save().await?;

    Ok((
        cookie_jar,
        url_builder.redirect(&mas_router::AccountRecoveryProgress::new(session.id)),
    )
        .into_response())
}

//:tchap:
/// Real function used when not testing
#[cfg(not(test))]
async fn check_email_allowed(
    email: &str,
    server_name: &str,
    tchap_config: &TchapConfig,
) -> Result<EmailAllowedResult, anyhow::Error> {
    tchap::is_email_allowed(email, server_name, tchap_config).await
}

/// Mock function used when testing
#[cfg(test)]
async fn check_email_allowed(
    email: &str,
    _server_name: &str,
    _tchap_config: &TchapConfig,
) -> Result<EmailAllowedResult, anyhow::Error> {
    if email == "wrong_server@example.com" {
        Ok(EmailAllowedResult::WrongServer {
            correct_server_name: "correct-server".to_owned(),
            wrong_server_name: "wrong-server".to_owned(),
        })
    } else {
        Ok(EmailAllowedResult::Allowed)
    }
}
//:tchap: end

//:tchap:
#[cfg(test)]
mod tests {
    use hyper::{Request, StatusCode, header::USER_AGENT};
    use sqlx::PgPool;

    use crate::test_utils::{CookieHelper, RequestBuilderExt, ResponseExt, TestState, setup};

    /// When the recovery email is mapped to a different server, the password
    /// reset request must be aborted before any session or email job is
    /// created, with a `wrong_server` error on the email field.
    #[sqlx::test(migrator = "mas_storage_pg::MIGRATOR")]
    async fn test_recovery_start_wrong_server_email(pool: PgPool) {
        setup();
        let state = TestState::from_pool(pool).await.unwrap();
        let cookies = CookieHelper::new();

        // Render the recovery start page to get a CSRF token
        let request = Request::get("/recover").empty();
        let request = cookies.with_cookies(request);
        let response = state.request(request).await;
        cookies.save_cookies(&response);
        response.assert_status(StatusCode::OK);

        // Extract the CSRF token
        let csrf_token = response
            .body()
            .split("name=\"csrf\" value=\"")
            .nth(1)
            .unwrap()
            .split('\"')
            .next()
            .unwrap();

        // Submit the recovery form with a wrong-server email
        let request = Request::post("/recover")
            .header(USER_AGENT, "test-agent")
            .form(serde_json::json!({
                "csrf": csrf_token,
                "email": "wrong_server@example.com",
            }));
        let request = cookies.with_cookies(request);
        let response = state.request(request).await;

        // Should be back on the recovery start page with the wrong_server error
        response.assert_status(StatusCode::OK);
        assert!(
            response.body().contains("wrong_server"),
            "expected wrong_server error in body: {}",
            response.body()
        );
    }
}
//:tchap: end
