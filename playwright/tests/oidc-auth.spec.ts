import { test, expect } from '../fixtures/auth-fixture';
import { 
  performOidcLogin, 
  verifyUserInMas, 
  createMasTestUser, 
  cleanupMasTestUser, 
  performPasswordLogin, 
  TestUser 
} from './utils/auth-helpers';
import { checkMasUserExistsByEmail } from './utils/mas-admin';

test.describe('Authentication Flows', () => {
  // Create a directory for screenshots
  test.beforeAll(async ({ }, testInfo) => {
    const { exec } = require('child_process');
    exec('mkdir -p playwright-results');
  });

  test('should authenticate via Keycloak and create user in MAS', async ({ page, testUser }) => {
    // Verify the test user doesn't exist in MAS yet
    const existsBeforeLogin = await checkMasUserExistsByEmail(testUser.email);
    expect(existsBeforeLogin).toBe(false);
    
    // Perform the OIDC login flow
    await performOidcLogin(page, testUser);
    
    // Click the create account button
    await page.locator('button[type="submit"]').click();

    // Verify we're successfully logged in
    // This could be checking for a specific element that's only visible when logged in
    await expect(page.locator('text=Sign out')).toBeVisible();
    
    // Take a screenshot of the authenticated state
    await page.screenshot({ path: 'playwright-results/04-authenticated.png' });
    
    // Verify the user was created in MAS
    await verifyUserInMas(testUser);
    
    // Double-check with the API
    const existsAfterLogin = await checkMasUserExistsByEmail(testUser.email);
    expect(existsAfterLogin).toBe(true);
    
    console.log(`Successfully authenticated and verified user ${testUser.username} (${testUser.email})`);
    console.log(`User ID in Keycloak: ${testUser.keycloakId}`);
    console.log(`User ID in MAS: ${testUser.masId}`);
  });
  
  test('should authenticate with username and password', async ({ page }) => {
    // Create a test user with a password in MAS
    const user = await createMasTestUser();
    
    try {
      console.log(`Created test user in MAS: ${user.username} (${user.email})`);
      
      // Perform password login
      await performPasswordLogin(page, user);
      
      // Verify we're successfully logged in
      await expect(page.locator('text=Sign out')).toBeVisible();
      
      // Take a screenshot of the authenticated state
      await page.screenshot({ path: 'playwright-results/04-password-auth-success.png' });
      
      console.log(`Successfully authenticated with password for user: ${user.username}`);
    } finally {
      // Clean up the test user
      await cleanupMasTestUser(user);
      console.log(`Cleaned up test user: ${user.username}`);
    }
  });
});
