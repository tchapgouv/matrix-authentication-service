// Copyright 2024 New Vector Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only
// Please see LICENSE in the repository root for full details.

//! # Migration
//!
//! This module provides the high-level logic for performing the Synapse-to-MAS
//! database migration.
//!
//! This module does not implement any of the safety checks that should be run
//! *before* the migration.

use std::{collections::HashMap, pin::pin};

use chrono::{DateTime, Utc};
use compact_str::CompactString;
use futures_util::StreamExt as _;
use mas_storage::Clock;
use rand::RngCore;
use thiserror::Error;
use thiserror_ext::ContextInto;
use tracing::Level;
use ulid::Ulid;
use uuid::Uuid;

use crate::{
    SynapseReader,
    mas_writer::{
        self, MasNewCompatAccessToken, MasNewCompatRefreshToken, MasNewCompatSession,
        MasNewEmailThreepid, MasNewUnsupportedThreepid, MasNewUpstreamOauthLink, MasNewUser,
        MasNewUserPassword, MasWriteBuffer, MasWriter,
    },
    synapse_reader::{
        self, ExtractLocalpartError, FullUserId, SynapseAccessToken, SynapseDevice,
        SynapseExternalId, SynapseRefreshableTokenPair, SynapseThreepid, SynapseUser,
    },
};

#[derive(Debug, Error, ContextInto)]
pub enum Error {
    #[error("error when reading synapse DB ({context}): {source}")]
    Synapse {
        source: synapse_reader::Error,
        context: String,
    },
    #[error("error when writing to MAS DB ({context}): {source}")]
    Mas {
        source: mas_writer::Error,
        context: String,
    },
    #[error("failed to extract localpart of {user:?}: {source}")]
    ExtractLocalpart {
        source: ExtractLocalpartError,
        user: FullUserId,
    },
    #[error("user {user} was not found for migration but a row in {table} was found for them")]
    MissingUserFromDependentTable { table: String, user: FullUserId },
    #[error(
        "missing a mapping for the auth provider with ID {synapse_id:?} (used by {user} and maybe other users)"
    )]
    MissingAuthProviderMapping {
        /// `auth_provider` ID of the provider in Synapse, for which we have no
        /// mapping
        synapse_id: String,
        /// a user that is using this auth provider
        user: FullUserId,
    },
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    struct UserFlags: u8 {
        const IS_SYNAPSE_ADMIN = 0b0000_0001;
        const IS_DEACTIVATED = 0b0000_0010;
        const IS_GUEST = 0b0000_0100;
    }
}

impl UserFlags {
    const fn is_deactivated(self) -> bool {
        self.contains(UserFlags::IS_DEACTIVATED)
    }

    const fn is_guest(self) -> bool {
        self.contains(UserFlags::IS_GUEST)
    }

    const fn is_synapse_admin(self) -> bool {
        self.contains(UserFlags::IS_SYNAPSE_ADMIN)
    }
}

#[derive(Debug, Clone, Copy)]
struct UserInfo {
    mas_user_id: Uuid,
    flags: UserFlags,
}

struct MigrationState {
    /// The server name we're migrating from
    server_name: String,

    /// Lookup table from user localpart to that user's infos
    users: HashMap<CompactString, UserInfo>,

    /// Mapping of MAS user ID + device ID to a MAS compat session ID.
    devices_to_compat_sessions: HashMap<(Uuid, CompactString), Uuid>,

    /// A mapping of Synapse external ID providers to MAS upstream OAuth 2.0
    /// provider ID
    provider_id_mapping: HashMap<String, Uuid>,
}

/// Performs a migration from Synapse's database to MAS' database.
///
/// # Panics
///
/// - If there are more than `usize::MAX` users
///
/// # Errors
///
/// Errors are returned under the following circumstances:
///
/// - An underlying database access error, either to MAS or to Synapse.
/// - Invalid data in the Synapse database.
#[allow(clippy::implicit_hasher)]
pub async fn migrate(
    synapse: &mut SynapseReader<'_>,
    mas: &mut MasWriter,
    server_name: String,
    clock: &dyn Clock,
    rng: &mut impl RngCore,
    provider_id_mapping: HashMap<String, Uuid>,
) -> Result<(), Error> {
    let counts = synapse.count_rows().await.into_synapse("counting users")?;

    let state = MigrationState {
        server_name,
        users: HashMap::with_capacity(counts.users),
        devices_to_compat_sessions: HashMap::with_capacity(counts.devices),
        provider_id_mapping,
    };

    let state = migrate_users(synapse, mas, state, rng).await?;
    let state = migrate_threepids(synapse, mas, rng, state).await?;
    let state = migrate_external_ids(synapse, mas, rng, state).await?;
    let state = migrate_unrefreshable_access_tokens(synapse, mas, clock, rng, state).await?;
    let state = migrate_refreshable_token_pairs(synapse, mas, clock, rng, state).await?;
    let _state = migrate_devices(synapse, mas, rng, state).await?;

    Ok(())
}

#[tracing::instrument(skip_all, level = Level::INFO)]
async fn migrate_users(
    synapse: &mut SynapseReader<'_>,
    mas: &mut MasWriter,
    mut state: MigrationState,
    rng: &mut impl RngCore,
) -> Result<MigrationState, Error> {
    let mut user_buffer = MasWriteBuffer::new(MasWriter::write_users);
    let mut password_buffer = MasWriteBuffer::new(MasWriter::write_passwords);
    let mut users_stream = pin!(synapse.read_users());

    while let Some(user_res) = users_stream.next().await {
        let user = user_res.into_synapse("reading user")?;
        let (mas_user, mas_password_opt) = transform_user(&user, &state.server_name, rng)?;

        let mut flags = UserFlags::empty();
        if bool::from(user.admin) {
            flags |= UserFlags::IS_SYNAPSE_ADMIN;
        }
        if bool::from(user.deactivated) {
            flags |= UserFlags::IS_DEACTIVATED;
        }
        if bool::from(user.is_guest) {
            flags |= UserFlags::IS_GUEST;
        }

        state.users.insert(
            CompactString::new(&mas_user.username),
            UserInfo {
                mas_user_id: mas_user.user_id,
                flags,
            },
        );

        user_buffer
            .write(mas, mas_user)
            .await
            .into_mas("writing user")?;

        if let Some(mas_password) = mas_password_opt {
            password_buffer
                .write(mas, mas_password)
                .await
                .into_mas("writing password")?;
        }
    }

    user_buffer.finish(mas).await.into_mas("writing users")?;
    password_buffer
        .finish(mas)
        .await
        .into_mas("writing passwords")?;

    Ok(state)
}

#[tracing::instrument(skip_all, level = Level::INFO)]
async fn migrate_threepids(
    synapse: &mut SynapseReader<'_>,
    mas: &mut MasWriter,
    rng: &mut impl RngCore,
    state: MigrationState,
) -> Result<MigrationState, Error> {
    let mut email_buffer = MasWriteBuffer::new(MasWriter::write_email_threepids);
    let mut unsupported_buffer = MasWriteBuffer::new(MasWriter::write_unsupported_threepids);
    let mut users_stream = pin!(synapse.read_threepids());

    while let Some(threepid_res) = users_stream.next().await {
        let SynapseThreepid {
            user_id: synapse_user_id,
            medium,
            address,
            added_at,
        } = threepid_res.into_synapse("reading threepid")?;
        let created_at: DateTime<Utc> = added_at.into();

        let username = synapse_user_id
            .extract_localpart(&state.server_name)
            .into_extract_localpart(synapse_user_id.clone())?
            .to_owned();
        let Some(user_infos) = state.users.get(username.as_str()).copied() else {
            if is_likely_appservice(&username) {
                continue;
            }
            return Err(Error::MissingUserFromDependentTable {
                table: "user_threepids".to_owned(),
                user: synapse_user_id,
            });
        };

        if medium == "email" {
            email_buffer
                .write(
                    mas,
                    MasNewEmailThreepid {
                        user_id: user_infos.mas_user_id,
                        user_email_id: Uuid::from(Ulid::from_datetime_with_source(
                            created_at.into(),
                            rng,
                        )),
                        email: address,
                        created_at,
                    },
                )
                .await
                .into_mas("writing email")?;
        } else {
            unsupported_buffer
                .write(
                    mas,
                    MasNewUnsupportedThreepid {
                        user_id: user_infos.mas_user_id,
                        medium,
                        address,
                        created_at,
                    },
                )
                .await
                .into_mas("writing unsupported threepid")?;
        }
    }

    email_buffer
        .finish(mas)
        .await
        .into_mas("writing email threepids")?;
    unsupported_buffer
        .finish(mas)
        .await
        .into_mas("writing unsupported threepids")?;

    Ok(state)
}

/// # Parameters
///
/// - `provider_id_mapping`: mapping from Synapse `auth_provider` ID to UUID of
///   the upstream provider in MAS.
#[tracing::instrument(skip_all, level = Level::INFO)]
async fn migrate_external_ids(
    synapse: &mut SynapseReader<'_>,
    mas: &mut MasWriter,
    rng: &mut impl RngCore,
    state: MigrationState,
) -> Result<MigrationState, Error> {
    let mut write_buffer = MasWriteBuffer::new(MasWriter::write_upstream_oauth_links);
    let mut extids_stream = pin!(synapse.read_user_external_ids());

    while let Some(extid_res) = extids_stream.next().await {
        let SynapseExternalId {
            user_id: synapse_user_id,
            auth_provider,
            external_id: subject,
        } = extid_res.into_synapse("reading external ID")?;
        let username = synapse_user_id
            .extract_localpart(&state.server_name)
            .into_extract_localpart(synapse_user_id.clone())?
            .to_owned();
        let Some(user_infos) = state.users.get(username.as_str()).copied() else {
            if is_likely_appservice(&username) {
                continue;
            }
            return Err(Error::MissingUserFromDependentTable {
                table: "user_external_ids".to_owned(),
                user: synapse_user_id,
            });
        };

        let Some(&upstream_provider_id) = state.provider_id_mapping.get(&auth_provider) else {
            return Err(Error::MissingAuthProviderMapping {
                synapse_id: auth_provider,
                user: synapse_user_id,
            });
        };

        // To save having to store user creation times, extract it from the ULID
        // This gives millisecond precision — good enough.
        let user_created_ts = Ulid::from(user_infos.mas_user_id).datetime();

        let link_id: Uuid = Ulid::from_datetime_with_source(user_created_ts, rng).into();

        write_buffer
            .write(
                mas,
                MasNewUpstreamOauthLink {
                    link_id,
                    user_id: user_infos.mas_user_id,
                    upstream_provider_id,
                    subject,
                    created_at: user_created_ts.into(),
                },
            )
            .await
            .into_mas("failed to write upstream link")?;
    }

    write_buffer
        .finish(mas)
        .await
        .into_mas("writing threepids")?;

    Ok(state)
}

/// Migrate devices from Synapse to MAS (as compat sessions).
///
/// In order to get the right session creation timestamps, the access tokens
/// must counterintuitively be migrated first, with the ULIDs passed in as
/// `devices`.
///
/// This is because only access tokens store a timestamp that in any way
/// resembles a creation timestamp.
#[tracing::instrument(skip_all, level = Level::INFO)]
async fn migrate_devices(
    synapse: &mut SynapseReader<'_>,
    mas: &mut MasWriter,
    rng: &mut impl RngCore,
    mut state: MigrationState,
) -> Result<MigrationState, Error> {
    let mut devices_stream = pin!(synapse.read_devices());
    let mut write_buffer = MasWriteBuffer::new(MasWriter::write_compat_sessions);

    while let Some(device_res) = devices_stream.next().await {
        let SynapseDevice {
            user_id: synapse_user_id,
            device_id,
            display_name,
            last_seen,
            ip,
            user_agent,
        } = device_res.into_synapse("reading Synapse device")?;

        let username = synapse_user_id
            .extract_localpart(&state.server_name)
            .into_extract_localpart(synapse_user_id.clone())?
            .to_owned();
        let Some(user_infos) = state.users.get(username.as_str()).copied() else {
            if is_likely_appservice(&username) {
                continue;
            }
            return Err(Error::MissingUserFromDependentTable {
                table: "devices".to_owned(),
                user: synapse_user_id,
            });
        };

        if user_infos.flags.is_deactivated() || user_infos.flags.is_guest() {
            continue;
        }

        let session_id = *state
            .devices_to_compat_sessions
            .entry((user_infos.mas_user_id, CompactString::new(&device_id)))
            .or_insert_with(||
                // We don't have a creation time for this device (as it has no access token),
                // so use now as a least-evil fallback.
                Ulid::with_source(rng).into());
        let created_at = Ulid::from(session_id).datetime().into();

        // As we're using a real IP type in the MAS database, it is possible
        // that we encounter invalid IP addresses in the Synapse database.
        // In that case, we should ignore them, but still log a warning.
        let last_active_ip = ip.and_then(|ip| {
            ip.parse()
                .map_err(|e| {
                    tracing::warn!(
                        error = &e as &dyn std::error::Error,
                        mxid = %synapse_user_id,
                        %device_id,
                        %ip,
                        "Failed to parse device IP, ignoring"
                    );
                })
                .ok()
        });

        write_buffer
            .write(
                mas,
                MasNewCompatSession {
                    session_id,
                    user_id: user_infos.mas_user_id,
                    device_id: Some(device_id),
                    human_name: display_name,
                    created_at,
                    is_synapse_admin: user_infos.flags.is_synapse_admin(),
                    last_active_at: last_seen.map(DateTime::from),
                    last_active_ip,
                    user_agent,
                },
            )
            .await
            .into_mas("writing compat sessions")?;
    }

    write_buffer
        .finish(mas)
        .await
        .into_mas("writing compat sessions")?;

    Ok(state)
}

/// Migrates unrefreshable access tokens (those without an associated refresh
/// token). Some of these may be deviceless.
#[tracing::instrument(skip_all, level = Level::INFO)]
async fn migrate_unrefreshable_access_tokens(
    synapse: &mut SynapseReader<'_>,
    mas: &mut MasWriter,
    clock: &dyn Clock,
    rng: &mut impl RngCore,
    mut state: MigrationState,
) -> Result<MigrationState, Error> {
    let mut token_stream = pin!(synapse.read_unrefreshable_access_tokens());
    let mut write_buffer = MasWriteBuffer::new(MasWriter::write_compat_access_tokens);
    let mut deviceless_session_write_buffer = MasWriteBuffer::new(MasWriter::write_compat_sessions);

    while let Some(token_res) = token_stream.next().await {
        let SynapseAccessToken {
            user_id: synapse_user_id,
            device_id,
            token,
            valid_until_ms,
            last_validated,
        } = token_res.into_synapse("reading Synapse access token")?;

        let username = synapse_user_id
            .extract_localpart(&state.server_name)
            .into_extract_localpart(synapse_user_id.clone())?
            .to_owned();
        let Some(user_infos) = state.users.get(username.as_str()).copied() else {
            if is_likely_appservice(&username) {
                continue;
            }
            return Err(Error::MissingUserFromDependentTable {
                table: "access_tokens".to_owned(),
                user: synapse_user_id,
            });
        };

        if user_infos.flags.is_deactivated() || user_infos.flags.is_guest() {
            continue;
        }

        // It's not always accurate, but last_validated is *often* the creation time of
        // the device If we don't have one, then use the current time as a
        // fallback.
        let created_at = last_validated.map_or_else(|| clock.now(), DateTime::from);

        let session_id = if let Some(device_id) = device_id {
            // Use the existing device_id if this is the second token for a device
            *state
                .devices_to_compat_sessions
                .entry((user_infos.mas_user_id, CompactString::new(&device_id)))
                .or_insert_with(|| {
                    Uuid::from(Ulid::from_datetime_with_source(created_at.into(), rng))
                })
        } else {
            // If this is a deviceless access token, create a deviceless compat session
            // for it (since otherwise we won't create one whilst migrating devices)
            let deviceless_session_id =
                Uuid::from(Ulid::from_datetime_with_source(created_at.into(), rng));

            deviceless_session_write_buffer
                .write(
                    mas,
                    MasNewCompatSession {
                        session_id: deviceless_session_id,
                        user_id: user_infos.mas_user_id,
                        device_id: None,
                        human_name: None,
                        created_at,
                        is_synapse_admin: false,
                        last_active_at: None,
                        last_active_ip: None,
                        user_agent: None,
                    },
                )
                .await
                .into_mas("failed to write deviceless compat sessions")?;

            deviceless_session_id
        };

        let token_id = Uuid::from(Ulid::from_datetime_with_source(created_at.into(), rng));

        write_buffer
            .write(
                mas,
                MasNewCompatAccessToken {
                    token_id,
                    session_id,
                    access_token: token,
                    created_at,
                    expires_at: valid_until_ms.map(DateTime::from),
                },
            )
            .await
            .into_mas("writing compat access tokens")?;
    }

    write_buffer
        .finish(mas)
        .await
        .into_mas("writing compat access tokens")?;
    deviceless_session_write_buffer
        .finish(mas)
        .await
        .into_mas("writing deviceless compat sessions")?;

    Ok(state)
}

/// Migrates (access token, refresh token) pairs.
/// Does not migrate non-refreshable access tokens.
#[tracing::instrument(skip_all, level = Level::INFO)]
async fn migrate_refreshable_token_pairs(
    synapse: &mut SynapseReader<'_>,
    mas: &mut MasWriter,
    clock: &dyn Clock,
    rng: &mut impl RngCore,
    mut state: MigrationState,
) -> Result<MigrationState, Error> {
    let mut token_stream = pin!(synapse.read_refreshable_token_pairs());
    let mut access_token_write_buffer = MasWriteBuffer::new(MasWriter::write_compat_access_tokens);
    let mut refresh_token_write_buffer =
        MasWriteBuffer::new(MasWriter::write_compat_refresh_tokens);

    while let Some(token_res) = token_stream.next().await {
        let SynapseRefreshableTokenPair {
            user_id: synapse_user_id,
            device_id,
            access_token,
            refresh_token,
            valid_until_ms,
            last_validated,
        } = token_res.into_synapse("reading Synapse refresh token")?;

        let username = synapse_user_id
            .extract_localpart(&state.server_name)
            .into_extract_localpart(synapse_user_id.clone())?
            .to_owned();
        let Some(user_infos) = state.users.get(username.as_str()).copied() else {
            if is_likely_appservice(&username) {
                continue;
            }
            return Err(Error::MissingUserFromDependentTable {
                table: "refresh_tokens".to_owned(),
                user: synapse_user_id,
            });
        };

        if user_infos.flags.is_deactivated() || user_infos.flags.is_guest() {
            continue;
        }

        // It's not always accurate, but last_validated is *often* the creation time of
        // the device If we don't have one, then use the current time as a
        // fallback.
        let created_at = last_validated.map_or_else(|| clock.now(), DateTime::from);

        // Use the existing device_id if this is the second token for a device
        let session_id = *state
            .devices_to_compat_sessions
            .entry((user_infos.mas_user_id, CompactString::new(&device_id)))
            .or_insert_with(|| Uuid::from(Ulid::from_datetime_with_source(created_at.into(), rng)));

        let access_token_id = Uuid::from(Ulid::from_datetime_with_source(created_at.into(), rng));
        let refresh_token_id = Uuid::from(Ulid::from_datetime_with_source(created_at.into(), rng));

        access_token_write_buffer
            .write(
                mas,
                MasNewCompatAccessToken {
                    token_id: access_token_id,
                    session_id,
                    access_token,
                    created_at,
                    expires_at: valid_until_ms.map(DateTime::from),
                },
            )
            .await
            .into_mas("writing compat access tokens")?;
        refresh_token_write_buffer
            .write(
                mas,
                MasNewCompatRefreshToken {
                    refresh_token_id,
                    session_id,
                    access_token_id,
                    refresh_token,
                    created_at,
                },
            )
            .await
            .into_mas("writing compat refresh tokens")?;
    }

    access_token_write_buffer
        .finish(mas)
        .await
        .into_mas("writing compat access tokens")?;

    refresh_token_write_buffer
        .finish(mas)
        .await
        .into_mas("writing compat refresh tokens")?;

    Ok(state)
}

fn transform_user(
    user: &SynapseUser,
    server_name: &str,
    rng: &mut impl RngCore,
) -> Result<(MasNewUser, Option<MasNewUserPassword>), Error> {
    let username = user
        .name
        .extract_localpart(server_name)
        .into_extract_localpart(user.name.clone())?
        .to_owned();

    let new_user = MasNewUser {
        user_id: Uuid::from(Ulid::from_datetime_with_source(
            DateTime::<Utc>::from(user.creation_ts).into(),
            rng,
        )),
        username,
        created_at: user.creation_ts.into(),
        locked_at: bool::from(user.deactivated).then_some(user.creation_ts.into()),
        can_request_admin: bool::from(user.admin),
        is_guest: bool::from(user.is_guest),
    };

    let mas_password = user
        .password_hash
        .clone()
        .map(|password_hash| MasNewUserPassword {
            user_password_id: Uuid::from(Ulid::from_datetime_with_source(
                DateTime::<Utc>::from(user.creation_ts).into(),
                rng,
            )),
            user_id: new_user.user_id,
            hashed_password: password_hash,
            created_at: new_user.created_at,
        });

    Ok((new_user, mas_password))
}

/// Returns true if and only if the given localpart looks like it would belong
/// to an application service user.
/// The rule here is that it must start with an underscore.
/// Synapse reserves these by default, but there is no hard rule prohibiting
/// other namespaces from being reserved, so this is not a robust check.
// TODO replace with a more robust mechanism, if we even care about this sanity check
// e.g. read application service registration files.
#[inline]
fn is_likely_appservice(localpart: &str) -> bool {
    localpart.starts_with('_')
}
