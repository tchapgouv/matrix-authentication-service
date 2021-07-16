// Copyright 2021 The Matrix.org Foundation C.I.C.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::ConfigurationSection;

fn default_http_address() -> String {
    "[::]:8080".into()
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct HttpConfig {
    #[serde(default = "default_http_address")]
    pub address: String,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            address: default_http_address(),
        }
    }
}

impl ConfigurationSection<'_> for HttpConfig {
    fn path() -> &'static str {
        "http"
    }

    fn generate() -> Self {
        Self::default()
    }
}
