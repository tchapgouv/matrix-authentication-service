// Copyright 2024, 2025 New Vector Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only
// Please see LICENSE in the repository root for full details.

fn main() -> anyhow::Result<()> {
    // Instruct rustc that we'll be using #[cfg(tokio_unstable)]
    println!("cargo::rustc-check-cfg=cfg(tokio_unstable)");

    Ok(())
}
