SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export MAS_HOME="$(dirname "$SCRIPT_DIR")"
cd $MAS_HOME

docker run --rm -v "$PWD/policies:/policies" docker.io/openpolicyagent/opa:1.8.0-debug  test  --schema /policies/schema/ --ignore schema /policies/.