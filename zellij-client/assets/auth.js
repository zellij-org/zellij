/**
 * Authentication logic and token management
 */

import { getBaseUrl } from "./utils.js";

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
            await showErrorModal(
                "Error",
                "Must provide security token in order to log in."
            );
        }
    }

    return { token, remember };
}

/**
 * Perform the login exchange with the server
 * @param {string} token - Authentication token
 * @param {boolean} rememberMe - Remember login preference
 * @returns {Promise<boolean>} true when the login succeeded
 */
async function login(token, rememberMe) {
    const baseUrl = getBaseUrl();
    let login_res = await fetch(`${baseUrl}/command/login`, {
        method: "POST",
        headers: {
            "Content-Type": "application/json",
        },
        body: JSON.stringify({
            auth_token: token,
            remember_me: rememberMe ? true : false,
        }),
        credentials: "include",
    });

    if (login_res.status === 401) {
        await showErrorModal("Error", "Unauthorized or revoked login token.");
        return false;
    } else if (!login_res.ok) {
        await showErrorModal(
            "Error",
            `Error ${login_res.status} connecting to server.`
        );
        return false;
    }
    return true;
}

/**
 * Request a session from the server, authenticating first if required.
 * @param {string} sessionFromPath - Session name taken from the URL path
 * @returns {Promise<Object>} The full session bootstrap payload
 */
export async function fetchSession(sessionFromPath) {
    const baseUrl = getBaseUrl();
    const query = sessionFromPath
        ? `?session=${encodeURIComponent(sessionFromPath)}`
        : "";

    while (true) {
        const response = await fetch(`${baseUrl}/session${query}`, {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
            },
            credentials: "include",
        });

        if (response.ok) {
            return await response.json();
        }

        if (response.status === 401) {
            const { token, remember } = await waitForSecurityToken();
            await login(token, remember);
            continue;
        }

        await showErrorModal(
            "Error",
            `Error ${response.status} connecting to server.`
        );
        const { token, remember } = await waitForSecurityToken();
        await login(token, remember);
    }
}
