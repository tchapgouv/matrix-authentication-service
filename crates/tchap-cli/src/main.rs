// Copyright 2024 New Vector Ltd.
// Copyright 2021-2024 The Matrix.org Foundation C.I.C.
//
// SPDX-License-Identifier: AGPL-3.0-only
// Please see LICENSE in the repository root for full details.

use std::{process::ExitCode, sync::Arc};

use mas_data_model::{AttributeMappingContext, CompiledConfig, UserMapper};
struct TchapUserMapper {}

#[async_trait::async_trait]
impl UserMapper for TchapUserMapper {
    async fn map_user(&self, context: &AttributeMappingContext) -> Option<String> {
        Some("".to_string())
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

    let mut compiled_config = CompiledConfig::new();
    compiled_config.add_user_mapper("tchap_user_mapper", Arc::new(TchapUserMapper {}));

    runtime.block_on(mas_cli::async_main(Some(compiled_config)))
}
