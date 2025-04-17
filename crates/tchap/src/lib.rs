extern crate tracing;
use tracing::{info, debug};


/// Capitalise parts of a name containing different words, including those
/// separated by hyphens.
///
/// For example, 'John-Doe'
///
/// # Parameters
///
/// * `name`: The name to parse
///
/// # Returns
///
/// The capitalized name
#[must_use]
pub fn cap(name: &str) -> String {
    if name.is_empty() {
        return name.to_string();
    }

    // Split the name by whitespace then hyphens, capitalizing each part then
    // joining it back together.
    let capitalized_name = name
        .split_whitespace()
        .map(|space_part| {
            space_part
                .split('-')
                .map(|part| {
                    let mut chars = part.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first_char) => {
                            let first_char_upper = first_char.to_uppercase().collect::<String>();
                            let rest: String = chars.collect();
                            format!("{}{}", first_char_upper, rest)
                        }
                    }
                })
                .collect::<Vec<String>>()
                .join("-")
        })
        .collect::<Vec<String>>()
        .join(" ");

    capitalized_name
}

/// Generate a Matrix ID localpart from an email address.
///
/// This function:
/// 1. Replaces "@" with "-" in the email address
/// 2. Converts the email to lowercase
/// 3. Filters out any characters that are not allowed in a Matrix ID localpart
///
/// The allowed characters are: lowercase ASCII letters, digits, and "_-./="
///
/// # Parameters
///
/// * `address`: The email address to process
///
/// # Returns
///
/// A valid Matrix ID localpart derived from the email address
#[must_use]
pub fn email_to_mxid_localpart(address: &str) -> String {
    // Define the allowed characters for a Matrix ID localpart
    const ALLOWED_CHARS: &str = "abcdefghijklmnopqrstuvwxyz0123456789_-./=";
    
    // Replace "@" with "-" and convert to lowercase
    let processed = address.replace('@', "-").to_lowercase();
    
    // Filter out any characters that are not allowed
    processed.chars().filter(|c| ALLOWED_CHARS.contains(*c)).collect()
}

/// Generate a display name from an email address based on specific rules.
///
/// This function:
/// 1. Replaces dots with spaces in the username part
/// 2. Determines the organization based on domain rules:
///    - gouv.fr emails use the subdomain or "gouv" if none
///    - other emails use the second-level domain
/// 3. Returns a display name in the format "Username [Organization]"
///
/// # Parameters
///
/// * `address`: The email address to process
///
/// # Returns
///
/// The formatted display name
#[must_use]
pub fn email_to_display_name(address: &str) -> String {
    // Split the part before and after the @ in the email.
    // Replace all . with spaces in the first part
    let parts: Vec<&str> = address.split('@').collect();
    if parts.len() != 2 {
        return String::new();
    }
    
    let username = parts[0].replace('.', " ");
    let domain = parts[1];

    // Figure out which org this email address belongs to
    let domain_parts: Vec<&str> = domain.split('.').collect();
    
    let org = if domain_parts.len() >= 2 && domain_parts[domain_parts.len() - 2] == "gouv" && domain_parts[domain_parts.len() - 1] == "fr" {
        // Is this is a ...gouv.fr address, set the org to whatever is before
        // gouv.fr. If there isn't anything (a @gouv.fr email) simply mark their
        // org as "gouv"
        if domain_parts.len() > 2 {
            domain_parts[domain_parts.len() - 3]
        } else {
            "gouv"
        }
    } else if domain_parts.len() >= 2 {
        // Otherwise, mark their org as the email's second-level domain name
        domain_parts[domain_parts.len() - 2]
    } else {
        ""
    };

    // Format the display name
    format!("{} [{}]", cap(&username), cap(org))
}

/// Checks if an email address is allowed to be associated in the current server
///
/// This function makes a synchronous GET request to the Matrix identity server API
/// to retrieve information about the home server associated with an email address,
/// then applies logic to determine if the email is allowed.
///
/// The API returns a JSON object with the following structure:
/// ```json
/// {
///   "hs": "string",
///   "requires_invite": true/false,
///   "invited": true/false
/// }
/// ```
///
/// # Parameters
///
/// * `email`: The email address to check
///
/// # Returns
///
/// A boolean indicating whether the email is allowed
#[must_use]
pub fn is_email_allowed(email: &str, server_name: &str) -> bool {
    // Construct the URL with the email address
    let url = format!(
        "http://localhost:8083/_matrix/identity/api/v1/info?medium=email&address={}",
        email
    );
    
    info!("Checking if email {} is allowed on server {}", email, server_name);
    info!("Making request to identity server: {}", url);

    // Make the HTTP request synchronously with a timeout
    match ureq::get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .call()
    {
        Ok(response) => {
            // Parse the JSON response
            match response.into_json::<serde_json::Value>() {
                Ok(json) => {


                    let hs = json.get("hs");

                    // Check if "hs" is in the response or if hs different from server_value
                    if !hs.is_some() || hs.unwrap() != server_name{
                        return false;
                    }
                    
                    info!("hs: {} ", hs.unwrap());

                    // Check if requires_invite is true and invited is false
                    let requires_invite = json.get("requires_invite")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                        
                    let invited = json.get("invited")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    
                    info!("requires_invite: {} invited: {}", requires_invite, invited);

                    if requires_invite && !invited {
                        // Requires an invite but hasn't been invited
                        return false;
                    }
                    
                    // All checks passed
                    true
                },
                Err(err) => {
                    // Log the error and return false
                    eprintln!("Failed to parse JSON response: {}", err);
                    false
                }
            }
        },
        Err(err) => {
            // Log the error and return false
            eprintln!("HTTP request failed: {}", err);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cap() {
        assert_eq!(cap("john"), "John");
        assert_eq!(cap("john-doe"), "John-Doe");
        assert_eq!(cap("john doe"), "John Doe");
        assert_eq!(cap("john-doe smith"), "John-Doe Smith");
        assert_eq!(cap(""), "");
    }

    #[test]
    fn test_email_to_display_name() {
        // Test gouv.fr email with subdomain
        assert_eq!(
            email_to_display_name("jane.smith@example.gouv.fr"),
            "Jane Smith [Example]"
        );

        // Test gouv.fr email without subdomain
        assert_eq!(
            email_to_display_name("user@gouv.fr"),
            "User [Gouv]"
        );

        // Test gouv.fr email with subdomain
        assert_eq!(
            email_to_display_name("user@gendarmerie.gouv.fr"),
            "User [Gendarmerie]"
        );

        // Test gouv.fr email with subdomain
        assert_eq!(
            email_to_display_name("user@gendarmerie.interieur.gouv.fr"),
            "User [Interieur]"
        );

        // Test regular email
        assert_eq!(
            email_to_display_name("contact@example.com"),
            "Contact [Example]"
        );

        // Test invalid email
        assert_eq!(email_to_display_name("invalid-email"), "");
    }

    #[test]
    fn test_email_to_mxid_localpart() {
        // Test basic email
        assert_eq!(
            email_to_mxid_localpart("john.doe@example.com"),
            "john.doe-example.com"
        );
        
        // Test with uppercase letters
        assert_eq!(
            email_to_mxid_localpart("John.Doe@Example.com"),
            "john.doe-example.com"
        );
        
        // Test with special characters
        assert_eq!(
            email_to_mxid_localpart("user+tag@domain.com"),
            "usertag-domain.com"
        );
        
        // Test with invalid characters
        assert_eq!(
            email_to_mxid_localpart("user!#$%^&*()@domain.com"),
            "user-domain.com"
        );
    }
}
