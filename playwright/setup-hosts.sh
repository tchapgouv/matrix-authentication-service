#!/bin/bash

# This script adds entries to the hosts file for local development
# It requires sudo privileges to modify the hosts file

# Check if running with sudo
if [ "$EUID" -ne 0 ]; then
  echo "Please run this script with sudo: sudo $0"
  exit 1
fi

# Hosts to add
HOSTS=(
  "127.0.0.1 auth.tchapgouv.com"
  "127.0.0.1 sso.tchapgouv.com"
)

# Path to hosts file
HOSTS_FILE="/etc/hosts"

echo "Adding hosts to $HOSTS_FILE..."

for HOST in "${HOSTS[@]}"; do
  if grep -q "$HOST" "$HOSTS_FILE"; then
    echo "Host $HOST already exists in $HOSTS_FILE"
  else
    echo "$HOST" >> "$HOSTS_FILE"
    echo "Added $HOST to $HOSTS_FILE"
  fi
done

echo "Done!"
echo "You can now access:"
echo "- Matrix Authentication Service at https://auth.tchapgouv.com"
echo "- Keycloak at https://sso.tchapgouv.com"
echo ""
echo "Note: You may need to flush your DNS cache for these changes to take effect."
echo "On macOS, run: sudo killall -HUP mDNSResponder"
echo "On Linux, run: sudo systemd-resolve --flush-caches"
