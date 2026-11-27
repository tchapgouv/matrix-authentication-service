set -e

echo "create admin.. needs MAS to be up"

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export MAS_HOME="$(dirname "$SCRIPT_DIR")"
export MAS_TCHAP_HOME=$SCRIPT_DIR
export RUST_LOG=info
echo "MAS_TCHAP_HOME " $MAS_TCHAP_HOME

export ADMIN_USERNAME="admin238987"


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