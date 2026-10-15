# :tchap: Hide the "continue with email" button for ProConnect-only domains.
package register

import rego.v1

# Domains provided as dynamic policy data (admin API: POST /api/admin/v1/policy-data):
#   data.register_proconnect_only_domains: list of regexes, full-match against the
#   lowercased email domain.

# METADATA
# schemas:
#   - input: schema["register_proconnect_only_input"]
# entrypoint: true
default proconnect_only := false

proconnect_only if {
	parts := split(input.email, "@")
	count(parts) == 2
	domain := lower(parts[1])
	some pattern in data.register_proconnect_only_domains
	regex.match(pattern, domain)
}
