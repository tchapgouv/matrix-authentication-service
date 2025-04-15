import { APIRequestContext, request } from '@playwright/test';
import { 
  KEYCLOAK_URL, 
  KEYCLOAK_ADMIN_USERNAME, 
  KEYCLOAK_ADMIN_PASSWORD,
  KEYCLOAK_REALM
} from './config';

// Create a reusable API request context
let apiContext: APIRequestContext | null = null;

async function getApiContext(): Promise<APIRequestContext> {
  if (!apiContext) {
    console.log(`[Keycloak API] Creating new API context with baseURL: ${KEYCLOAK_URL}`);
    apiContext = await request.newContext({
      baseURL: KEYCLOAK_URL,
      ignoreHTTPSErrors: true
    });
  }
  return apiContext;
}

/**
 * Get an admin access token for Keycloak
 */
export async function getKeycloakAdminToken(): Promise<string> {
  console.log(`[Keycloak API] Requesting admin token with username: ${KEYCLOAK_ADMIN_USERNAME}`);
  const apiRequestContext = await getApiContext();
  
  const response = await apiRequestContext.post('/realms/master/protocol/openid-connect/token', {
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    form: {
      grant_type: 'password',
      client_id: 'admin-cli',
      username: KEYCLOAK_ADMIN_USERNAME,
      password: KEYCLOAK_ADMIN_PASSWORD
    }
  });

  if (!response.ok()) {
    const errorText = await response.text();
    console.error(`[Keycloak API] Failed to get admin token: ${response.status()} - ${errorText}`);
    throw new Error(`Failed to get Keycloak admin token: ${response.status()} - ${errorText}`);
  }

  const data = await response.json() as { access_token: string };
  console.log(`[Keycloak API] Successfully obtained admin token`);
  return data.access_token;
}

/**
 * Create a user in Keycloak
 */
export async function createKeycloakUser(username: string, email: string, password: string): Promise<string> {
  console.log(`[Keycloak API] Creating user: ${username} with email: ${email}`);
  const token = await getKeycloakAdminToken();
  const apiRequestContext = await getApiContext();
  
  // First, create the user
  console.log(`[Keycloak API] Creating user in realm: ${KEYCLOAK_REALM}`);
  const createResponse = await apiRequestContext.post(`/admin/realms/${KEYCLOAK_REALM}/users`, {
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${token}`
    },
    data: {
      username,
      email,
      enabled: true,
      emailVerified: true,
      firstName: username,
      lastName: username,
      attributes: {
        idp_id: username
      }
    }
  });

  if (!createResponse.ok()) {
    const errorText = await createResponse.text();
    console.error(`[Keycloak API] Failed to create user: ${createResponse.status()} - ${errorText}`);
    throw new Error(`Failed to create Keycloak user: ${createResponse.status()} - ${errorText}`);
  }
  console.log(`[Keycloak API] User created successfully`);

  // Get the user ID
  console.log(`[Keycloak API] Getting user ID for username: ${username}`);
  const usersResponse = await apiRequestContext.get(
    `/admin/realms/${KEYCLOAK_REALM}/users?username=${encodeURIComponent(username)}`,
    {
      headers: {
        'Authorization': `Bearer ${token}`
      }
    }
  );

  if (!usersResponse.ok()) {
    const errorText = await usersResponse.text();
    console.error(`[Keycloak API] Failed to get user ID: ${usersResponse.status()} - ${errorText}`);
    throw new Error(`Failed to get Keycloak user: ${usersResponse.status()} - ${errorText}`);
  }

  const users = await usersResponse.json() as Array<{ id: string }>;
  if (users.length === 0) {
    console.error(`[Keycloak API] User ${username} not found after creation`);
    throw new Error(`User ${username} not found after creation`);
  }

  const userId = users[0].id;
  console.log(`[Keycloak API] Found user with ID: ${userId}`);

  // Set the password
  console.log(`[Keycloak API] Setting password for user: ${username}`);
  const passwordResponse = await apiRequestContext.put(
    `/admin/realms/${KEYCLOAK_REALM}/users/${userId}/reset-password`,
    {
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`
      },
      data: {
        type: 'password',
        value: password,
        temporary: false
      }
    }
  );

  if (!passwordResponse.ok()) {
    const errorText = await passwordResponse.text();
    console.error(`[Keycloak API] Failed to set password: ${passwordResponse.status()} - ${errorText}`);
    throw new Error(`Failed to set Keycloak user password: ${passwordResponse.status()} - ${errorText}`);
  }
  console.log(`[Keycloak API] Password set successfully for user: ${username}`);

  return userId;
}

/**
 * Delete a user from Keycloak
 */
export async function deleteKeycloakUser(userId: string): Promise<void> {
  console.log(`[Keycloak API] Deleting user with ID: ${userId}`);
  const token = await getKeycloakAdminToken();
  const apiRequestContext = await getApiContext();
  
  const response = await apiRequestContext.delete(
    `/admin/realms/${KEYCLOAK_REALM}/users/${userId}`,
    {
      headers: {
        'Authorization': `Bearer ${token}`
      }
    }
  );

  if (!response.ok()) {
    const errorText = await response.text();
    console.error(`[Keycloak API] Failed to delete user: ${response.status()} - ${errorText}`);
    throw new Error(`Failed to delete Keycloak user: ${response.status()} - ${errorText}`);
  }
  console.log(`[Keycloak API] User deleted successfully`);
}

/**
 * Check if a user exists in Keycloak
 */
export async function checkKeycloakUserExists(username: string): Promise<boolean> {
  console.log(`[Keycloak API] Checking if user exists: ${username}`);
  const token = await getKeycloakAdminToken();
  const apiRequestContext = await getApiContext();
  
  const response = await apiRequestContext.get(
    `/admin/realms/${KEYCLOAK_REALM}/users?username=${encodeURIComponent(username)}`,
    {
      headers: {
        'Authorization': `Bearer ${token}`
      }
    }
  );

  if (!response.ok()) {
    const errorText = await response.text();
    console.error(`[Keycloak API] Failed to check user: ${response.status()} - ${errorText}`);
    throw new Error(`Failed to check Keycloak user: ${response.status()} - ${errorText}`);
  }

  const users = await response.json() as Array<{ id: string }>;
  const exists = users.length > 0;
  console.log(`[Keycloak API] User ${username} exists: ${exists}`);
  return exists;
}

/**
 * Dispose the API context when done
 */
export async function disposeApiContext(): Promise<void> {
  if (apiContext) {
    console.log(`[Keycloak API] Disposing API context`);
    await apiContext.dispose();
    apiContext = null;
    console.log(`[Keycloak API] API context disposed`);
  } else {
    console.log(`[Keycloak API] No API context to dispose`);
  }
}
