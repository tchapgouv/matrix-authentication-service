import { test as base } from '@playwright/test';
import { createKeycloakTestUser, cleanupKeycloakTestUser, TestUser } from '../tests/utils/auth-helpers';
import { disposeApiContext as disposeKeycloakApiContext } from '../tests/utils/keycloak-admin';
import { disposeApiContext as disposeMasApiContext } from '../tests/utils/mas-admin';

/**
 * Extend the basic test fixtures with our authentication fixtures
 */
export const test = base.extend<{
  testUser: TestUser;
}>({
  /**
   * Create a test user in Keycloak before the test and clean it up after
   */
  testUser: async ({}, use) => {
    try {
      // Create a test user in Keycloak
      const user = await createKeycloakTestUser();
      console.log(`Created test user: ${user.username} (${user.email})`);
      
      // Use the test user in the test
      await use(user);
      
      // Clean up the test user after the test
      await cleanupKeycloakTestUser(user);
      console.log(`Cleaned up test user: ${user.username}`);
    } finally {
      // Dispose API contexts
      await Promise.all([
        disposeKeycloakApiContext(),
        disposeMasApiContext()
      ]);
      console.log('API contexts disposed');
    }
  },
});

export { expect } from '@playwright/test';
