// Copyright 2025, 2026 Element Creations Ltd.
// Copyright 2024, 2025 New Vector Ltd.
// Copyright 2024 The Matrix.org Foundation C.I.C.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE files in the repository root for full details.

use anyhow::Context;
use async_trait::async_trait;
use mas_email::{Address, Mailbox};
use mas_i18n::DataLocale;
use mas_storage::{
    Pagination, RepositoryAccess,
    queue::SendAccountRecoveryEmailsJob,
    user::{UserEmailFilter, UserRecoveryRepository},
};
use mas_templates::{EmailRecoveryContext, TemplateContext};
use rand::distributions::{Alphanumeric, DistString};
use tracing::{error, info};

use crate::{
    State,
    new_queue::{JobContext, JobError, RunnableJob},
};

/// Job to send account recovery emails for a given recovery session.
#[async_trait]
impl RunnableJob for SendAccountRecoveryEmailsJob {
    #[tracing::instrument(
        name = "job.send_account_recovery_email",
        fields(
            user_recovery_session.id = %self.user_recovery_session_id(),
            user_recovery_session.email,
        ),
        skip_all,
    )]
    async fn run(&self, state: &State, _context: JobContext) -> Result<(), JobError> {
        let clock = state.clock();
        let mailer = state.mailer();
        let url_builder = state.url_builder();
        let mut rng = state.rng();
        let mut repo = state.repository().await.map_err(JobError::retry)?;

        let session = repo
            .user_recovery()
            .lookup_session(self.user_recovery_session_id())
            .await
            .map_err(JobError::retry)?
            .context("User recovery session not found")
            .map_err(JobError::fail)?;

        tracing::Span::current().record("user_recovery_session.email", &session.email);

        if session.consumed_at.is_some() {
            info!("Recovery session already consumed, not sending email");
            return Ok(());
        }

        let mut cursor = Pagination::first(50);

        let lang: DataLocale = session
            .locale
            .parse()
            .context("Invalid locale in database on recovery session")
            .map_err(JobError::fail)?;

        loop {
            let page = repo
                .user_email()
                .list(UserEmailFilter::new().for_email(&session.email), cursor)
                .await
                .map_err(JobError::retry)?;

            //:tchap:
            // If no UserEmail is found in database, try to find a deactivated
            // user by its generated username
            if page.edges.is_empty() {
                // a deactivated account does not get a recovery ticket: it
                // receives a specific "account deactivated" email instead.
                // search by generated username
                let user = repo
                    .user()
                    .find_by_username(&tchap::email_to_mxid_localpart(&session.email))
                    .await
                    .map_err(JobError::retry)?;

                // only a deactivated user gets the "account deactivated" email
                if let Some(user) = user.filter(|user| user.deactivated_at.is_some()) {
                    info!(user.id = %user.id, "User is deactivated");

                    tchap::send_deactivated_recovery_email(mailer, &user, &session.email, lang)
                        .await;
                    break;
                }
            }
            //:tchap: end

            for edge in page.edges {
                let ticket = Alphanumeric.sample_string(&mut rng, 32);

                let ticket = repo
                    .user_recovery()
                    .add_ticket(&mut rng, clock, &session, &edge.node, ticket)
                    .await
                    .map_err(JobError::retry)?;

                let user = repo
                    .user()
                    .lookup(edge.node.user_id)
                    .await
                    .map_err(JobError::retry)?
                    .context("User not found")
                    .map_err(JobError::fail)?;

                let url = url_builder.account_recovery_link(ticket.ticket);

                let address: Address = edge.node.email.parse().map_err(JobError::fail)?;
                let mailbox = Mailbox::new(Some(user.username.clone()), address);

                info!("Sending recovery email to {}", mailbox);
                let context =
                    EmailRecoveryContext::new(user, session.clone(), url).with_language(lang);

                // XXX: we only log if the email fails to send, to avoid stopping the loop
                if let Err(e) = mailer.send_recovery_email(mailbox, &context).await {
                    error!(
                        error = &e as &dyn std::error::Error,
                        "Failed to send recovery email"
                    );
                }

                cursor = cursor.after(edge.cursor);
            }

            if !page.has_next_page {
                break;
            }
        }

        repo.save().await.map_err(JobError::fail)?;

        Ok(())
    }
}
