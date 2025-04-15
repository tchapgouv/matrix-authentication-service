#!/bin/bash

set -e

# Run mail service
# docker rm -f mailcatcher
# docker run -d --name mailcatcher -p 1080:1080 -p 1025:1025 sj26/mailcatcher
# Run postgres service
#createdb -U postgres postgres
# dropdb postgres
docker rm -f postgres
docker run -d --name postgres -p 5432:5432 -v ./tchap/postgres:/var/lib/postgresql/data:rw -e 'POSTGRES_USER=postgres' -e 'POSTGRES_PASSWORD=postgres' -e 'POSTGRES_DATABASE=postgres' -e 'PGDATA=/var/lib/postgresql/data' postgres
#createdb -U postgres -O postgres keycloak
#docker run -d -p 5432:5432 -e 'POSTGRES_USER=keycloak' -e 'POSTGRES_PASSWORD=keycloak' -e 'POSTGRES_DATABASE=keycloak' postgres-keycloak
# docker exec -it postgres createdb -U postgres keycloak

# Run Keycloak service with proconnect-mock realm import
# frontend url : https://sso.tchapgouv.com
docker rm -f keycloak
docker run -d --name keycloak -p 8082:8080 -e KEYCLOAK_ADMIN=admin -e KEYCLOAK_ADMIN_PASSWORD=admin -v $(pwd)/tchap/keycloak/proconnect-mock-realm.json:/opt/keycloak/data/import/proconnect-mock-realm.json quay.io/keycloak/keycloak:latest start-dev --import-realm --hostname=https://sso.tchapgouv.com
#cargo run -- server -c config.local.dev.yaml
# cargo test --package mas-handlers upstream_oauth2::link::tests
