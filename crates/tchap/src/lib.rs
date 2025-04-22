extern crate tracing;
use tracing::info;

use serde::{Deserialize, Serialize};
use url::Url;

mod identity_client;

/// Configuration for Tchap-specific functionality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TchapConfig {
    /// The base URL of the identity server API
    pub identity_server_url: Url,
}

fn default_identity_server_url() -> Url {
    // Try to read the TCHAP_IDENTITY_SERVER_URL environment variable
    match std::env::var("TCHAP_IDENTITY_SERVER_URL") {
        Ok(url_str) => {
            // Attempt to parse the URL from the environment variable
            match Url::parse(&url_str) {
                Ok(url) => {
                    // Success: use the URL from the environment variable
                    return url;
                }
                Err(err) => {
                    // Parsing error: log a warning and use the default value
                    tracing::warn!(
                        "The TCHAP_IDENTITY_SERVER_URL environment variable contains an invalid URL: {}. Using default value.",
                        err
                    );
                }
            }
        }
        Err(std::env::VarError::NotPresent) => {
            // Variable not defined: use the default value without warning
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            // Variable contains non-Unicode characters: log a warning
            tracing::warn!(
                "The TCHAP_IDENTITY_SERVER_URL environment variable contains non-Unicode characters. Using default value."
            );
        }
    }
    
    // Default value if the environment variable is not defined or invalid
    Url::parse("http://localhost:8083").unwrap()
}

impl Default for TchapConfig {
    fn default() -> Self {
        Self {
            identity_server_url: default_identity_server_url(),
        }
    }
}

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
    // Get the identity server URL from the environment variable or use the default
    let identity_server_url = default_identity_server_url();
    
    // Create the client and get the URL using the identity_client module
    let (url, client) = identity_client::create_identity_client(
        email, 
        server_name, 
        identity_server_url
    );

    // Make the HTTP request asynchronously
    match client.get(&url).send().await {
        Ok(response) => {
            // Parse the JSON response
            match response.json::<serde_json::Value>().await {
                Ok(json) => {
                    let hs = json.get("hs");

                    // Check if "hs" is in the response or if hs different from server_value
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
                    eprintln!("Failed to parse JSON response: {}", err);
                    EmailAllowedResult::WrongServer
                }
            }
        }
        Err(err) => {
            // Log the error and return WrongServer as a default error
            eprintln!("HTTP request failed: {}", err);
            EmailAllowedResult::WrongServer
        }
    }
}
