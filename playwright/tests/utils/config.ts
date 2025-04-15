import dotenv from 'dotenv';

// Load environment variables from .env file
dotenv.config();

// URLs
export const MAS_URL = process.env.MAS_URL || 'https://auth.tchapgouv.com';
export const KEYCLOAK_URL = process.env.KEYCLOAK_URL || 'https://sso.tchapgouv.com';

// Keycloak Admin Credentials
export const KEYCLOAK_ADMIN_USERNAME = process.env.KEYCLOAK_ADMIN_USERNAME || 'admin';
export const KEYCLOAK_ADMIN_PASSWORD = process.env.KEYCLOAK_ADMIN_PASSWORD || 'admin';
export const KEYCLOAK_REALM = process.env.KEYCLOAK_REALM || 'proconnect-mock';
export const KEYCLOAK_CLIENT_ID = process.env.KEYCLOAK_CLIENT_ID || 'mas';

// MAS Admin API Credentials
export const MAS_ADMIN_CLIENT_ID = process.env.MAS_ADMIN_CLIENT_ID || '01J44RKQYM4G3TNVANTMTDYTX6';
export const MAS_ADMIN_CLIENT_SECRET = process.env.MAS_ADMIN_CLIENT_SECRET || 'phoo8ahneir3ohY2eigh4xuu6Oodaewi';

// Test User Credentials
export const TEST_USER_PREFIX = process.env.TEST_USER_PREFIX || 'playwright_test_user';
export const TEST_USER_PASSWORD = process.env.TEST_USER_PASSWORD || 'Test@123456';

// Generate a unique username and email for testing
export function generateTestUser() {
  const timestamp = new Date().getTime();
  const randomSuffix = Math.floor(Math.random() * 10000);
  const username = `${TEST_USER_PREFIX}_${timestamp}_${randomSuffix}`;
  const email = `${username}@example.com`;
  
  return {
    username,
    email,
    password: TEST_USER_PASSWORD
  };
}
