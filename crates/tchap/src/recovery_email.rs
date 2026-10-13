//! Send a specific email when a deactivated account requests a password reset.

use mas_data_model::User;
use mas_email::{Address, Mailbox, Mailer};
use mas_i18n::DataLocale;
use mas_templates::{EmailRecoveryDeactivatedContext, TemplateContext};

/// If the given user is deactivated, send the "account deactivated" email
/// instead of the standard recovery email.
///
/// Returns `true` if the user is deactivated (no recovery ticket must be
/// created), `false` otherwise.
pub async fn send_deactivated_recovery_email(
    mailer: &Mailer,
    user: &User,
    email: &str,
    lang: DataLocale,
) -> bool {
    let address: Address = match email.parse() {
        Ok(address) => address,
        Err(e) => {
            tracing::error!(
                error = &e as &dyn std::error::Error,
                %email,
                "Invalid email address for deactivated account"
            );
            // Still consider the user as handled: no recovery ticket must be
            // created for a deactivated account
            return true;
        }
    };
    let mailbox = Mailbox::new(Some(user.username.clone()), address);

    tracing::info!(user.id = %user.id, "Sending deactivated account email to {}", mailbox);
    let context = EmailRecoveryDeactivatedContext::new(user.clone()).with_language(lang);

    if let Err(e) = mailer
        .send_recovery_deactivated_email(mailbox, &context)
        .await
    {
        tracing::error!(
            error = &e as &dyn std::error::Error,
            "Failed to send deactivated account email"
        );
    }

    true
}
