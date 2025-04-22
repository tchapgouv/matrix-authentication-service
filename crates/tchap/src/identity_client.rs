//! This module provides utilities for interacting with the Matrix identity server API.

use reqwest;
use std::time::Duration;
use tracing::info;
use url::Url;

/// Creates a client for the identity server and constructs the URL for the info endpoint
///
/// # Parameters
///
/// * `email`: The email address to check
/// * `server_name`: The name of the server to check against
/// * `identity_server_url`: The base URL of the identity server
///
/// # Returns
///
/// A tuple containing the constructed URL and the HTTP client
pub fn create_identity_client(
    email: &str,
    server_name: &str,
    identity_server_url: Url,
) -> (String, reqwest::Client) {
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

    (url, client)
}
