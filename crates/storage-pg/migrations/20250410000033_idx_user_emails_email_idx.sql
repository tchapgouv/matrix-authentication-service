-- no-transaction
-- Copyright 2025 New Vector Ltd.
--
-- SPDX-License-Identifier: AGPL-3.0-only
-- Please see LICENSE in the repository root for full details.

-- This isn't a foreign key, but we really need that to be indexed
CREATE INDEX CONCURRENTLY
  user_emails_email_idx
  ON user_emails (email);
