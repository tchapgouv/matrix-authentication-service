#!/bin/bash

set -e

# Run mail service
docker rm -f mailcatcher
docker run -d --name mailcatcher -p 1080:1080 -p 1025:1025 sj26/mailcatcher
# Run postgres service
#createdb -U postgres postgres
# dropdb postgres
docker rm -f postgres
docker run -d --name postgres -p 5432:5432 -v ./tchap/postgres:/var/lib/postgresql/data:rw -e 'POSTGRES_USER=postgres' -e 'POSTGRES_PASSWORD=postgres' -e 'POSTGRES_DATABASE=postgres' -e 'PGDATA=/var/lib/postgresql/data' postgres
#createdb -U postgres -O postgres keycloak
#docker run -d -p 5432:5432 -e 'POSTGRES_USER=keycloak' -e 'POSTGRES_PASSWORD=keycloak' -e 'POSTGRES_DATABASE=keycloak' postgres-keycloak
#cargo run -- server -c config.local.dev.yaml
# cargo test --package mas-handlers upstream_oauth2::link::tests