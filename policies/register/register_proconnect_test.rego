# :tchap: Tests for the ProConnect-only registration policy.
package register_test

import data.register
import rego.v1

test_proconnect_only_matches_domain if {
	register.proconnect_only with input as {"email": "Jean.Dupont@ac-versailles.fr"}
		with data.register_proconnect_only_domains as [`ac-versailles\.fr`]
}

test_proconnect_only_matches_regex if {
	register.proconnect_only with input as {"email": "a@foo.education.fr"}
		with data.register_proconnect_only_domains as [`.*\.education\.fr`]
}

test_proconnect_only_no_match if {
	not register.proconnect_only with input as {"email": "a@example.com"}
		with data.register_proconnect_only_domains as [`ac-versailles\.fr`]
}

test_proconnect_only_empty_list if {
	not register.proconnect_only with input as {"email": "a@ac-versailles.fr"}
		with data.register_proconnect_only_domains as []
}

test_proconnect_only_no_data if {
	not register.proconnect_only with input as {"email": "a@ac-versailles.fr"}
}

test_proconnect_only_invalid_email if {
	not register.proconnect_only with input as {"email": "not-an-email"}
		with data.register_proconnect_only_domains as [`ac-versailles\.fr`]
}
