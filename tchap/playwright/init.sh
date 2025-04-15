#!/bin/bash

# This script initializes the Playwright test project

# Create the results directory
mkdir -p playwright-results

# Install dependencies
echo "Installing dependencies..."
rm -rf node_modules
npm install

# Install Playwright browsers
echo "Installing Playwright browsers..."
npx playwright install

echo "Initialization complete!"
echo ""
echo "Next steps:"
echo "1. Configure hosts with: sudo ./setup-hosts.sh"
echo "2. Start the services with: ./tchap/start-local-stack.sh and ./tchap/start-local-mas.sh"
echo "3. Run the tests with: npm test"
echo ""
echo "For more information, see the README.md file."
