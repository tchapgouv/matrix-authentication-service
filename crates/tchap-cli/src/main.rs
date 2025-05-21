// Copyright 2024 New Vector Ltd.
// Copyright 2021-2024 The Matrix.org Foundation C.I.C.
//
// SPDX-License-Identifier: AGPL-3.0-only
// Please see LICENSE in the repository root for full details.

use std::{collections::HashMap, process::ExitCode, sync::Arc};

use mas_cli::app_state::UserMapper;

struct TchapUserMapper {
}

#[async_trait::async_trait]
impl UserMapper for TchapUserMapper {
    async fn map_user(&self, user_id: &str) -> Option<String> {
        Some(user_id.to_string())
    }
}

fn main() -> anyhow::Result<ExitCode> {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all();

    #[cfg(tokio_unstable)]
    builder
        .enable_metrics_poll_time_histogram()
        .metrics_poll_time_histogram_configuration(tokio::runtime::HistogramConfiguration::log(
            tokio::runtime::LogHistogram::default(),
        ));

    let runtime = builder.build()?;

    let mut user_mappers: HashMap<String, Arc<dyn UserMapper>> = HashMap::new();
    user_mappers.insert("tchap_user_mapper".to_string(), Arc::new(TchapUserMapper{}));

    runtime.block_on(mas_cli::async_main(Some(mas_cli::CompiledConfig {
        user_mappers,
    })))
}