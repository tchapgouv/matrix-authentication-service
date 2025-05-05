// Copyright 2025 New Vector Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only
// Please see LICENSE in the repository root for full details.

use std::collections::HashSet;

use crate::{HomeserverConnection, MatrixUser, ProvisionRequest};

/// A wrapper around a [`HomeserverConnection`] that only allows read
/// operations.
pub struct ReadOnlyHomeserverConnection<C> {
    inner: C,
}

impl<C> ReadOnlyHomeserverConnection<C> {
    pub fn new(inner: C) -> Self
    where
        C: HomeserverConnection,
    {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl<C: HomeserverConnection> HomeserverConnection for ReadOnlyHomeserverConnection<C> {
    fn homeserver(&self) -> &str {
        self.inner.homeserver()
    }

    async fn query_user(&self, mxid: &str) -> Result<MatrixUser, anyhow::Error> {
        self.inner.query_user(mxid).await
    }

    async fn provision_user(&self, _request: &ProvisionRequest) -> Result<bool, anyhow::Error> {
        anyhow::bail!("Provisioning is not supported in read-only mode");
    }

    async fn is_localpart_available(&self, localpart: &str) -> Result<bool, anyhow::Error> {
        self.inner.is_localpart_available(localpart).await
    }

    async fn create_device(
        &self,
        _mxid: &str,
        _device_id: &str,
        _initial_display_name: Option<&str>,
    ) -> Result<(), anyhow::Error> {
        anyhow::bail!("Device creation is not supported in read-only mode");
    }

    async fn update_device_display_name(
        &self,
        _mxid: &str,
        _device_id: &str,
        _display_name: &str,
    ) -> Result<(), anyhow::Error> {
        anyhow::bail!("Device display name update is not supported in read-only mode");
    }

    async fn delete_device(&self, _mxid: &str, _device_id: &str) -> Result<(), anyhow::Error> {
        anyhow::bail!("Device deletion is not supported in read-only mode");
    }

    async fn sync_devices(
        &self,
        _mxid: &str,
        _devices: HashSet<String>,
    ) -> Result<(), anyhow::Error> {
        anyhow::bail!("Device synchronization is not supported in read-only mode");
    }

    async fn delete_user(&self, _mxid: &str, _erase: bool) -> Result<(), anyhow::Error> {
        anyhow::bail!("User deletion is not supported in read-only mode");
    }

    async fn reactivate_user(&self, _mxid: &str) -> Result<(), anyhow::Error> {
        anyhow::bail!("User reactivation is not supported in read-only mode");
    }

    async fn set_displayname(&self, _mxid: &str, _displayname: &str) -> Result<(), anyhow::Error> {
        anyhow::bail!("User displayname update is not supported in read-only mode");
    }

    async fn unset_displayname(&self, _mxid: &str) -> Result<(), anyhow::Error> {
        anyhow::bail!("User displayname update is not supported in read-only mode");
    }

    async fn allow_cross_signing_reset(&self, _mxid: &str) -> Result<(), anyhow::Error> {
        anyhow::bail!("Allowing cross-signing reset is not supported in read-only mode");
    }
}
