extern crate tracing;
use tracing::info;

mod identity_client;

/// Result of checking if an email is allowed on a server
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailAllowedResult {
    /// Email is allowed on this server
    Allowed,
    /// Email is mapped to a different server
    WrongServer,
    /// Server requires an invitation that is not present
    InvitationMissing,
}

/// Checks if an email address is allowed to be associated in the current server
///
/// This function makes an asynchronous GET request to the Matrix identity server API
/// to retrieve information about the home server associated with an email address,
/// then applies logic to determine if the email is allowed.
///
/// # Parameters
///
/// * `email`: The email address to check
/// * `server_name`: The name of the server to check against
///
/// # Returns
///
/// An `EmailAllowedResult` indicating whether the email is allowed and if not, why
#[must_use]
pub async fn is_email_allowed(email: &str, server_name: &str) -> EmailAllowedResult {
    // Query the identity server
    match identity_client::query_identity_server(email).await {
        Ok(json) => {
            let hs = json.get("hs");

            // Check if "hs" is in the response or if hs different from server_name
            if !hs.is_some() || hs.unwrap() != server_name {
                // Email is mapped to a different server or no server at all
                return EmailAllowedResult::WrongServer;
            }

            info!("hs: {} ", hs.unwrap());

            // Check if requires_invite is true and invited is false
            let requires_invite = json
                .get("requires_invite")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let invited = json
                .get("invited")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            info!("requires_invite: {} invited: {}", requires_invite, invited);

            if requires_invite && !invited {
                // Requires an invite but hasn't been invited
                return EmailAllowedResult::InvitationMissing;
            }

            // All checks passed
            EmailAllowedResult::Allowed
        }
        Err(err) => {
            // Log the error and return WrongServer as a default error
            eprintln!("HTTP request failed: {}", err);
            EmailAllowedResult::WrongServer
        }
    }
}
