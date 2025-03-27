#!/bin/sh

set -eu


# ```sh
# # Install the CLI
# cargo install sqlx-cli --no-default-features --features postgres

# cd crates/storage-pg/ # Must be in the mas-storage-pg crate folder
# export DATABASE_URL=postgresql:///matrix_auth
# cargo sqlx prepare
# ```

# ## Migrations

# Migration files live in the `migrations` folder in the `mas-core` crate.

# ```sh
# cd crates/storage-pg/ # Again, in the mas-storage-pg crate folder
# export DATABASE_URL=postgresql:///matrix_auth
# cargo sqlx migrate run # Run pending migrations
# cargo sqlx migrate add [description] # Add new migration files
# ```

# cd ..
# ./misc/update.sh
# cargo +nightly fmt 
# export DATABASE_URL=postgresql://postgres:postgres@localhost/postgres
# cargo test --workspace
# unset DATABASE_URL
# cargo clippy --workspace --tests --bins --lib -- -D warnings