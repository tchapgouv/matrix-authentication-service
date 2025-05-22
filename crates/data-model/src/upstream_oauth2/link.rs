// Copyright 2024 New Vector Ltd.
// Copyright 2023, 2024 The Matrix.org Foundation C.I.C.
//
// SPDX-License-Identifier: AGPL-3.0-only
// Please see LICENSE in the repository root for full details.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use ulid::Ulid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpstreamOAuthLink {
    pub id: Ulid,
    pub provider_id: Ulid,
    pub user_id: Option<Ulid>,
    pub subject: String,
    pub human_account_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Context passed to the attribute mapping template
///
/// The variables available in the template are:
/// - `user`: claims for the user, merged from the ID token and userinfo
///   endpoint
/// - `id_token_claims`: claims from the ID token
/// - `userinfo_claims`: claims from the userinfo endpoint
/// - `extra_callback_parameters`: extra parameters passed to the callback
#[derive(Debug, Default)]
pub struct AttributeMappingContext {
    pub id_token_claims: Option<HashMap<String, serde_json::Value>>,
    pub extra_callback_parameters: Option<serde_json::Value>,
    pub userinfo_claims: Option<serde_json::Value>,
}

impl AttributeMappingContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_id_token_claims(
        mut self,
        id_token_claims: HashMap<String, serde_json::Value>,
    ) -> Self {
        self.id_token_claims = Some(id_token_claims);
        self
    }

    pub fn with_extra_callback_parameters(
        mut self,
        extra_callback_parameters: serde_json::Value,
    ) -> Self {
        self.extra_callback_parameters = Some(extra_callback_parameters);
        self
    }

    pub fn with_userinfo_claims(mut self, userinfo_claims: serde_json::Value) -> Self {
        self.userinfo_claims = Some(userinfo_claims);
        self
    }
}

#[async_trait::async_trait]
pub trait UserMapper: Send + Sync {
    async fn map_user(&self, context: &AttributeMappingContext) -> Option<String>;
}
