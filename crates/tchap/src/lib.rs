extern crate tracing;
use tracing::info;

use reqwest;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

/// Configuration for Tchap-specific functionality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TchapConfig {
    /// The base URL of the identity server API
    pub identity_server_url: Url,
}

fn default_identity_server_url() -> Url {
    // Essayer de lire la variable d'environnement TCHAP_IDENTITY_SERVER_URL
    match std::env::var("TCHAP_IDENTITY_SERVER_URL") {
        Ok(url_str) => {
            // Tenter de parser l'URL depuis la variable d'environnement
            match Url::parse(&url_str) {
                Ok(url) => {
                    // Succès : utiliser l'URL de la variable d'environnement
                    return url;
                }
                Err(err) => {
                    // Erreur de parsing : logger un avertissement et utiliser la valeur par défaut
                    tracing::warn!(
                        "La variable d'environnement TCHAP_IDENTITY_SERVER_URL contient une URL invalide : {}. Utilisation de la valeur par défaut.",
                        err
                    );
                }
            }
        }
        Err(std::env::VarError::NotPresent) => {
            // Variable non définie : utiliser la valeur par défaut sans avertissement
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            // Variable contient des caractères non-Unicode : logger un avertissement
            tracing::warn!(
                "La variable d'environnement TCHAP_IDENTITY_SERVER_URL contient des caractères non-Unicode. Utilisation de la valeur par défaut."
            );
        }
    }

    // Valeur par défaut si la variable d'environnement n'est pas définie ou invalide
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

    // Construct the URL with the email address
    let url = format!(
        "{}_matrix/identity/api/v1/info?medium=email&address={}",
        identity_server_url, email
    );

    info!(
        "Checking if email {} is allowed on server {}",
        email, server_name
    );
    info!("Making request to identity server: {}", url);

    // Create a client with a timeout
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default();

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
