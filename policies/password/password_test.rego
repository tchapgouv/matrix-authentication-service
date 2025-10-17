package password_test

import data.password
import rego.v1

test_password_too_short if {
	not password.allow with input as {"value": "short"}
}

test_password_long_enough if {
	password.allow with input as {"value": "longenoughpassword"}
}