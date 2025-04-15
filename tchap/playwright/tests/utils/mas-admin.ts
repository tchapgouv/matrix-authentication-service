import { APIRequestContext, request } from '@playwright/test';
import { 
  MAS_URL, 
  MAS_ADMIN_CLIENT_ID, 
  MAS_ADMIN_CLIENT_SECRET 
} from './config';

// Create a reusable API request context
let apiContext: APIRequestContext | null = null;

async function getApiContext(): Promise<APIRequestContext> {
  if (!apiContext) {
    console.log(`[MAS API] Creating new API context with baseURL: ${MAS_URL}`);
    apiContext = await request.newContext({
      baseURL: MAS_URL,
      ignoreHTTPSErrors: true
    });
  }
  return apiContext;
}

/**
 * Get an admin access token for MAS
 */
export async function getMasAdminToken(): Promise<string> {
  console.log(`[MAS API] Requesting admin token with client ID: ${MAS_ADMIN_CLIENT_ID}`);
  const apiRequestContext = await getApiContext();
  const authHeader = Buffer.from(`${MAS_ADMIN_CLIENT_ID}:${MAS_ADMIN_CLIENT_SECRET}`).toString('base64');
  
  const response = await apiRequestContext.post('/oauth2/token', {
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
      'Authorization': `Basic ${authHeader}`
    },
    form: {
      grant_type: 'client_credentials',
      scope: 'urn:mas:admin'
    }
  });

  if (!response.ok()) {
    const errorText = await response.text();
    console.error(`[MAS API] Failed to get admin token: ${response.status()} - ${errorText}`);
    throw new Error(`Failed to get MAS admin token: ${response.status()} - ${errorText}`);
  }

  const data = await response.json() as { access_token: string };
  console.log(`[MAS API] Successfully obtained admin token`);
  return data.access_token;
}

/**
 * Check if a user exists in MAS by email
 */
export async function checkMasUserExistsByEmail(email: string): Promise<boolean> {
  console.log(`[MAS API] Checking if user exists with email: ${email}`);
  const token = await getMasAdminToken();
  const apiRequestContext = await getApiContext();
  
  const response = await apiRequestContext.get(
    `/api/admin/v1/user-emails?filter[email]=${encodeURIComponent(email)}`,
    {
      headers: {
        'Authorization': `Bearer ${token}`
      }
    }
  );

  if (!response.ok()) {
    const errorText = await response.text();
    console.error(`[MAS API] Failed to check user: ${response.status()} - ${errorText}`);
    throw new Error(`Failed to check MAS user: ${response.status()} - ${errorText}`);
  }

  const result = await response.json() as { data: Array<{ id: string }> };
  const exists = result.data.length > 0;
  console.log(`[MAS API] User with email ${email} exists: ${exists}`);
  return exists;
}

/**
 * Get user details from MAS by email
 */
export async function getMasUserByEmail(email: string): Promise<any | null> {
  console.log(`[MAS API] Getting user details for email: ${email}`);
  const token = await getMasAdminToken();
  const apiRequestContext = await getApiContext();
  
  const response = await apiRequestContext.get(
    `/api/admin/v1/user-emails?filter[email]=${encodeURIComponent(email)}`,
    {
      headers: {
        'Authorization': `Bearer ${token}`
      }
    }
  );

  if (!response.ok()) {
    const errorText = await response.text();
    console.error(`[MAS API] Failed to get user details: ${response.status()} - ${errorText}`);
    throw new Error(`Failed to get MAS user: ${response.status()} - ${errorText}`);
  }

  const result = await response.json() as { data: Array<any> };
  const user = result.data.length > 0 ? result.data[0] : null;
  console.log(`[MAS API] User found: ${user !== null ? 'Yes' : 'No'}`);
  if (user) {
    console.log(`[MAS API] User ID: ${user.id}, Username: ${user.attributes?.username || 'N/A'}`);
  }
  return user;
}

/**
 * Wait for a user to be created in MAS
 * This is useful after OIDC authentication, as there might be a slight delay
 * before the user is fully created in MAS
 */
export async function waitForMasUser(email: string, maxAttempts = 10, delayMs = 1000): Promise<any> {
  console.log(`[MAS API] Waiting for user with email ${email} to be created (max ${maxAttempts} attempts)`);
  for (let attempt = 0; attempt < maxAttempts; attempt++) {
    console.log(`[MAS API] Attempt ${attempt + 1}/${maxAttempts} to find user`);
    try {
      const user = await getMasUserByEmail(email);
      if (user) {
        console.log(`[MAS API] User found on attempt ${attempt + 1}`);
        return user;
      }
      console.log(`[MAS API] User not found on attempt ${attempt + 1}, waiting ${delayMs}ms before next attempt`);
    } catch (error) {
      console.warn(`[MAS API] Attempt ${attempt + 1}/${maxAttempts} failed: ${error}`);
    }
    
    // Wait before the next attempt
    await new Promise(resolve => setTimeout(resolve, delayMs));
  }
  
  const errorMsg = `User with email ${email} not found in MAS after ${maxAttempts} attempts`;
  console.error(`[MAS API] ${errorMsg}`);
  throw new Error(errorMsg);
}

/**
 * Create a user in MAS with a password
 */
export async function createMasUserWithPassword(username: string, email: string, password: string): Promise<string> {
  console.log(`[MAS API] Creating user with password: ${username} (${email})`);
  const token = await getMasAdminToken();
  const apiRequestContext = await getApiContext();
  
  const response = await apiRequestContext.post('/api/admin/v1/users', {
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${token}`
    },
    data: {
      "username": username,
      "skip_homeserver_check": false
    }
  });
  
  if (!response.ok()) {
    const errorText = await response.text();
    console.error(`[MAS API] Failed to create user: ${response.status()} - ${errorText}`);
    throw new Error(`Failed to create MAS user: ${response.status()} - ${errorText}`);
  }

  const data = await response.json();
  console.log(data.data)
  const userId = data.data.id;

  const responsePwd = await apiRequestContext.post(`/api/admin/v1/users/${userId}/set-password`, {
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${token}`
    },
    data: {
      "password": password,
      "skip_password_check": true
    }
  });

  if (!responsePwd.ok()) {
    const errorText = await responsePwd.text();
    console.error(`[MAS API] Failed to set password for user: ${responsePwd.status()} - ${errorText}`);
    throw new Error(`Failed to set password for user: ${responsePwd.status()} - ${errorText}`);
  }
  
  console.log(`[MAS API] User created successfully with ID: ${userId}`);
  return userId;
}

/**
 * Delete a user from MAS
 */
export async function deactivateMasUser(userId: string): Promise<void> {
  console.log(`[MAS API] Deleting user with ID: ${userId}`);
  const token = await getMasAdminToken();
  const apiRequestContext = await getApiContext();
  
  const response = await apiRequestContext.post(`/api/admin/v1/users/${userId}/deactivate`, {
    headers: {
      'Authorization': `Bearer ${token}`
    }
  });
  
  if (!response.ok()) {
    const errorText = await response.text();
    console.error(`[MAS API] Failed to delete user: ${response.status()} - ${errorText}`);
    throw new Error(`Failed to delete MAS user: ${response.status()} - ${errorText}`);
  }
  
  console.log(`[MAS API] User deleted successfully`);
}

/**
 * Dispose the API context when done
 */
export async function disposeApiContext(): Promise<void> {
  if (apiContext) {
    console.log(`[MAS API] Disposing API context`);
    await apiContext.dispose();
    apiContext = null;
    console.log(`[MAS API] API context disposed`);
  } else {
    console.log(`[MAS API] No API context to dispose`);
  }
}
