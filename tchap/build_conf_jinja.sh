#!/bin/bash

set -e

echo "Starting Jinja2 configuration build process with jinja2-cli..."

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Set paths relative to script location
TCHAP_HOME="$SCRIPT_DIR"
MAS_HOME="$(dirname "$TCHAP_HOME")"
TCHAP_ENV_YAML="$TCHAP_HOME/.env.yaml"
MAS_TCHAP_DATA="$MAS_TCHAP_HOME/tmp"

echo "Step 1/3: Checking requirements..."
# Check if Docker is available
if ! command -v docker &> /dev/null; then
  echo "Error: Docker is not installed or not in PATH."
  exit 1
fi

echo "Step 2/3: Validating variables file..."
# Check if .env.yaml file exists
if [ ! -f "$TCHAP_ENV_YAML" ]; then
  echo "Error: .env.yaml file not found at $TCHAP_ENV_YAML"
  echo "Please create a .env.yaml file based on .env"
  exit 1
fi

echo "Step 3/3: Rendering Jinja2 template..."
# Create tmp directory if needed
MAS_TCHAP_DATA="$TCHAP_HOME/tmp"
if [ ! -d "$MAS_TCHAP_DATA" ]; then
  mkdir -p "$MAS_TCHAP_DATA"
fi


MAS_TCHAP_TRANSLATIONS="$MAS_HOME/tchap/resources/translations"
MAS_TCHAP_TEMPLATES="$MAS_TCHAP_DATA/templates"


# Run jinja2-cli via Docker using official image from mattrobenolt
# Using .env.yaml format for proper YAML parsing of complex variables
docker run --rm \
  -v "$TCHAP_HOME/conf/config.yaml.j2:/template.j2:ro" \
  -v "$TCHAP_ENV_YAML:/.env.yaml:ro" \
  -v "$MAS_TCHAP_DATA:/output" \
  -e MAS_TCHAP_TRANSLATIONS=$MAS_TCHAP_TRANSLATIONS \
  -e MAS_TCHAP_TEMPLATES=$MAS_TCHAP_TEMPLATES \
  ghcr.io/mattrobenolt/jinja2:main \
  /template.j2 /.env.yaml -o /output/config.local.dev.yaml

if [ -f "$MAS_TCHAP_DATA/config.local.dev.yaml" ]; then
  echo "✓ Configuration file generated successfully!"
  echo "  Output: $MAS_TCHAP_DATA/config.local.dev.yaml"
else
  echo "Error: Failed to generate configuration file."
  exit 1
fi

echo "Configuration build completed successfully!"
