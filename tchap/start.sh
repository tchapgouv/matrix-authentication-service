#!/bin/bash

# start the MAS

set -e

echo "Starting MAS initialization process..."

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export MAS_HOME="$(dirname "$SCRIPT_DIR")"
export MAS_TCHAP_HOME=$SCRIPT_DIR
export RUST_LOG=info
echo "MAS_TCHAP_HOME " $MAS_TCHAP_HOME

export ADMIN_USERNAME="admin238987"

echo ""
if [[ "$*" == *"-w"* ]]; then
    CONFIG_FILE="config.local.dev-worker.yaml"
    TCHAP_ENV_YAML="$MAS_TCHAP_HOME/.env.worker.yaml"
    NO_WORKER_FLAG=""
    SERVER_COMMAND="worker"
else
    CONFIG_FILE="config.local.dev.yaml"
    TCHAP_ENV_YAML="$MAS_TCHAP_HOME/.env.yaml"
    #NO_WORKER_FLAG="--no-worker"
    SERVER_COMMAND="server"
fi

DOCKER_COMPOSE_FILE="$MAS_TCHAP_HOME/docker-compose.yml"

# web listener port (used killing old processes)
PORT_WEB=$(sed -nE 's/^[[:space:]]*port_web:[[:space:]]*//p' "$TCHAP_ENV_YAML" | head -n 1)
if ! [[ "$PORT_WEB" =~ ^[0-9]+$ ]]; then
    echo "Warning: could not read port_web from $TCHAP_ENV_YAML, defaulting to 8080"
    PORT_WEB=8080
fi

echo "Step 0: Kill previous processes at port:$PORT_WEB..."
PIDS="$(lsof -ti:"$PORT_WEB" || true)"
if [ -n "$PIDS" ]; then
    kill $PIDS
fi



# start the postgres service if not running already
echo "Step 1/7: Checking PostgreSQL service status..."

# Check if postgres container is running
# NB: use -f so the check works regardless of the current directory
if ! docker compose -f $DOCKER_COMPOSE_FILE ps postgres | grep -q "Up"; then
    echo "PostgreSQL is not running. Starting docker-compose services..."
    docker compose -f $DOCKER_COMPOSE_FILE up -d postgres

    # Wait for PostgreSQL to be ready.
    # NB: this script runs with `set -e`, so the readiness check MUST live in
    # the `until` condition: a standalone `pg_isready` call aborts the whole
    # script on the first "no response" instead of waiting.
    echo "Waiting for PostgreSQL to be ready..."
    MAX_WAIT=300
    WAITED=0
    until docker compose -f $DOCKER_COMPOSE_FILE exec -T postgres pg_isready -U postgres > /dev/null 2>&1; do
        if [ "$WAITED" -ge "$MAX_WAIT" ]; then
            echo "Error: PostgreSQL was not ready after ${MAX_WAIT}s" >&2
            exit 1
        fi
        echo "PostgreSQL not ready yet (waited ${WAITED}s)..."
        sleep 5
        WAITED=$((WAITED + 5))
    done
    set -e
    echo "PostgreSQL is ready!"
else
    echo "PostgreSQL is already running."
fi

echo "Step 2/7: Checking policy.wasm..."
#if policy.wasm does not exists create it 
if [ -f $MAS_HOME/policies/policy.wasm ]; then
    echo "policy.wasm found" 
else
    cd $MAS_HOME/policies
    echo "Creating policy.wasm..."
    make DOCKER=1
fi

cd "$MAS_HOME/frontend"

# Check if -f flag is present in command line arguments
if [[ "$*" == *"-f"* ]]; then
    echo "Step 3/7: Installing npm dependencies..."
    pnpm install
    echo "Step 4/7: Building frontend and static resources..."
    pnpm run build-tchap
else
    echo "Skipping npm install and build steps due to absence of -f flag"
fi



# Internal listener port (used for the health check below), read from the env YAML
PORT_INTERNAL=$(sed -nE 's/^[[:space:]]*port_internal:[[:space:]]*//p' "$TCHAP_ENV_YAML" | head -n 1)
if ! [[ "$PORT_INTERNAL" =~ ^[0-9]+$ ]]; then
    echo "Warning: could not read port_internal from $TCHAP_ENV_YAML, defaulting to 8081"
    PORT_INTERNAL=8081
fi

echo "Step 5/7: Building configuration..."
$MAS_TCHAP_HOME/build_conf_jinja.sh $TCHAP_ENV_YAML $CONFIG_FILE

cd "$MAS_HOME"


echo "Step 6/7: Checking templates..."
cargo run -- templates check -c $MAS_TCHAP_HOME/tmp/$CONFIG_FILE

echo "Step 7/7: Starting server..."
cargo run -- $SERVER_COMMAND -c $MAS_TCHAP_HOME/tmp/$CONFIG_FILE $NO_WORKER_FLAG
SERVER_PID=$!

# The issue-compatibility-token command below requires the server to be up:
# poll the health endpoint until the server answers.
if [ "$SERVER_COMMAND" = "server" ]; then
    HEALTH_URL="http://localhost:${PORT_INTERNAL}/health"
    echo "Waiting for the MAS server to be healthy ($HEALTH_URL)..."
    MAX_WAIT=300  # 
    WAITED=0
    until curl -sf --max-time 2 -o /dev/null "$HEALTH_URL"; do
        if ! kill -0 "$SERVER_PID" 2>/dev/null; then
            echo "Warning: the MAS server process exited before becoming healthy. Continuing anyway..."
            break
        fi
        if [ "$WAITED" -ge "$MAX_WAIT" ]; then
            echo "Warning: timed out after ${MAX_WAIT}s waiting for the MAS server. Continuing anyway..."
            break
        fi
        sleep 2
        WAITED=$((WAITED + 2))
    done
    if curl -sf --max-time 2 -o /dev/null "$HEALTH_URL"; then
        echo "MAS server is up."
    fi

    cargo run -- manage register-user $ADMIN_USERNAME --admin --yes  \
    --ignore-password-complexity --password "admin" --email "$ADMIN_USERNAME@tchapgouv.com" \
    -c $MAS_TCHAP_HOME/tmp/config.local.dev.yaml  \
    || echo "Warning: register-user failed (user '$ADMIN_USERNAME' may already exist). Continuing anyway..."


    # This command requires the server to be up
    cargo run -- manage issue-compatibility-token $ADMIN_USERNAME --yes-i-want-to-grant-synapse-admin-privileges \
        -c $MAS_TCHAP_HOME/tmp/config.local.dev.yaml \
        || echo "Warning: issue-compatibility-token failed. Continuing anyway..."
fi

echo "MAS initialization completed successfully!"
