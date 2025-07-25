// Copyright 2025 New Vector Ltd.
//
// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
// Please see LICENSE files in the repository root for full details.

mod add;
mod delete;
mod get;
mod list;

pub use self::{
    add::{doc as add_doc, handler as add},
    delete::{doc as delete_doc, handler as delete},
    get::{doc as get_doc, handler as get},
    list::{doc as list_doc, handler as list},
};
