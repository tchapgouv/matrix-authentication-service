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
echo "1. Start the services with: ./tchap/start-local-stack.sh and ./tchap/start-local-mas.sh"
echo "2. Run the tests with: npm test"
echo ""
echo "For more information, see the README.md file."
