/**
 * Authentication logic and token management
 */

import { is_https } from './utils.js';

/**
 * Wait for user to provide a security token
 * @returns {Promise<{token: string, remember: boolean}>}
 */
async function waitForSecurityToken() {
    let token = null;
    let remember = null;
    
    while (!token) {
        let result = await getSecurityToken();
        if (result) {
            token = result.token;
            remember = result.remember;
        } else {
            await showErrorModal("Error", "Must provide security token in order to log in.");
        }
    }
    
    return { token, remember };
}

/**
 * Get client ID from server after authentication
 * @param {string} token - Authentication token
 * @param {boolean} rememberMe - Remember login preference
 * @param {boolean} hasAuthenticationCookie - Whether auth cookie exists
 * @returns {Promise<string|null>} Client ID or null on failure
 */
export async function getClientId(token, rememberMe, hasAuthenticationCookie) {
    let url_prefix = is_https() ? "https" : "http";
    
    if (!hasAuthenticationCookie) {
        let login_res = await fetch(`${url_prefix}://${window.location.host}/command/login`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({
                auth_token: token,
                remember_me: rememberMe ? true : false
            }),
            credentials: 'include'
        });

        if (login_res.status === 401) {
            await showErrorModal("Error", "Unauthorized or revoked login token.");
            return null;
        } else if (!login_res.ok) {
            await showErrorModal("Error", `Error ${login_res.status} connecting to server.`);
            return null;
        }
    }
    
    let data = await fetch(`${url_prefix}://${window.location.host}/session`, {
        method: "POST",
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({}),
    });
    
    if (data.status === 401) {
        await showErrorModal("Error", "Unauthorized or revoked login token.");
        return null;
    } else if (!data.ok) {
        await showErrorModal("Error", `Error ${data.status} connecting to server.`);
        return null;
    } else {
        let body = await data.json();
        return body.web_client_id;
    }
}

/**
 * Initialize authentication flow and return client ID
 * @returns {Promise<string>} Client ID
 */
export async function initAuthentication() {
    let token = null;
    let remember = null;
    let hasAuthenticationCookie = window.is_authenticated;
    
    if (!hasAuthenticationCookie) {
        const tokenResult = await waitForSecurityToken();
        token = tokenResult.token;
        remember = tokenResult.remember;
    }
    
    let webClientId;
    
    while (!webClientId) {
        webClientId = await getClientId(token, remember, hasAuthenticationCookie);
        if (!webClientId) {
            hasAuthenticationCookie = false;
            const tokenResult = await waitForSecurityToken();
            token = tokenResult.token;
            remember = tokenResult.remember;
        }
    }
    
    return webClientId;
}
