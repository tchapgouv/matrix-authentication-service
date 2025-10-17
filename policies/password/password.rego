# Copyright 2025 New Vector Ltd.
#
# SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-Element-Commercial
# Please see LICENSE files in the repository root for full details.

# METADATA
# schemas:
#   - input: schema["password_input"]
package password

import rego.v1

default allow := false

allow if {
	count(violation) == 0
}

# METADATA
# entrypoint: true
violation contains {"field": "password", "code": "password-too-short", "msg": "minimum 12 caractères"} if {
    count(input.value) < 12
}
