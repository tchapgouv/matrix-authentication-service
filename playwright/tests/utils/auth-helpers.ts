import { Page } from '@playwright/test';
import { createKeycloakUser, deleteKeycloakUser } from './keycloak-admin';
import { waitForMasUser, createMasUserWithPassword, deactivateMasUser } from './mas-admin';
import { generateTestUser } from './config';

/**
 * Test user type
 */
export interface TestUser {
  username: string;
  email: string;
  password: string;
  keycloakId?: string;
  masId?: string;
}

/**
 * Create a test user in Keycloak
 */
export async function createKeycloakTestUser(): Promise<TestUser> {
  const user = generateTestUser();
  const keycloakId = await createKeycloakUser(user.username, user.email, user.password);
  return { ...user, keycloakId };
}

/**
 * Clean up a test user
 */
export async function cleanupKeycloakTestUser(user: TestUser): Promise<void> {
  if (user.keycloakId) {
    await deleteKeycloakUser(user.keycloakId);
  }
}

/**
 * Perform OIDC login via Keycloak
 * This function handles the entire authentication flow:
 * 1. Navigate to MAS login page
 * 2. Click on the OIDC provider button
 * 3. Fill in credentials on the Keycloak login page
 * 4. Wait for successful authentication and redirect back to MAS
 */
export async function performOidcLogin(page: Page, user: TestUser): Promise<void> {
  // Navigate to the login page
  await page.goto('/login');
  
  // Take a screenshot of the login page
  await page.screenshot({ path: 'playwright-results/01-login-page.png' });
  
  // Find and click the OIDC provider button (adjust the selector as needed)
  // This is based on the login.html template which shows provider buttons
  const oidcButton = page.locator('a.cpd-button[href*="/upstream/authorize/"]');
  await oidcButton.click();
  
  // Wait for navigation to Keycloak
  await page.waitForURL(url => url.toString().includes('sso.tchapgouv.com'));
  
  // Take a screenshot of the Keycloak login page
  await page.screenshot({ path: 'playwright-results/02-keycloak-login.png' });
  
  // Fill in the username and password
  await page.locator('#username').fill(user.username);
  await page.locator('#password').fill(user.password);
  
  // Click the login button
  await page.locator('button[type="submit"]').click();
  

  // Wait for redirect back to MAS
  await page.waitForURL(url => url.toString().includes('auth.tchapgouv.com'));
  
  // Take a screenshot after successful login
  await page.screenshot({ path: 'playwright-results/03-after-login.png' });
}

/**
 * Verify that a user was created in MAS after OIDC authentication
 */
export async function verifyUserInMas(user: TestUser): Promise<void> {
  const masUser = await waitForMasUser(user.email);
  user.masId = masUser.id;
}

/**
 * Create a test user directly in MAS with password
 */
export async function createMasTestUser(): Promise<TestUser> {
  const user = generateTestUser();
  const masId = await createMasUserWithPassword(user.username, user.email, user.password);
  return { ...user, masId };
}

/**
 * Clean up a MAS test user
 */
export async function cleanupMasTestUser(user: TestUser): Promise<void> {
  if (user.masId) {
    await deactivateMasUser(user.masId);
  }
}

/**
 * Perform password login to MAS
 * This function handles the direct authentication flow:
 * 1. Navigate to MAS login page
 * 2. Fill in username and password
 * 3. Submit the form
 * 4. Wait for successful authentication
 */
export async function performPasswordLogin(page: Page, user: TestUser): Promise<void> {
  console.log(`[Auth] Performing password login for user: ${user.username}`);
  
  // Navigate to the login page
  await page.goto('/login');
  
  // Take a screenshot of the login page
  await page.screenshot({ path: 'playwright-results/01-password-login-page.png' });
  
  // Fill in the username and password
  await page.locator('input[name="username"]').fill(user.username);
  await page.locator('input[name="password"]').fill(user.password);
  
  // Take a screenshot before submitting
  await page.screenshot({ path: 'playwright-results/02-password-login-filled.png' });
  
  // Click the login button (submit the form)
  await page.locator('button[type="submit"]').click();
  
  // Wait for successful login (redirect to dashboard or home page)
  await page.waitForURL(url => !url.toString().includes('/login'));
  
  // Take a screenshot after successful login
  await page.screenshot({ path: 'playwright-results/03-password-login-success.png' });
  
  console.log(`[Auth] Password login successful for user: ${user.username}`);
}
