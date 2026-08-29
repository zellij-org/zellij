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

let pendingAuthentication = null;
let authenticationGeneration = 0;

/**
 * Authenticate, coalescing concurrent callers onto a single prompt.
 *
 * Any number of requests may be rejected at once - the session menu polls while
 * the page is still booting, for example. Each one asking the user separately
 * would stack a dialog per rejection, so the first caller owns the prompt and
 * the rest await its result. They all resume together once it settles, and
 * retry against the credentials it established.
 *
 * @returns {Promise<boolean>} true when the login succeeded
 */
function authenticate() {
    if (!pendingAuthentication) {
        pendingAuthentication = (async () => {
            try {
                const { token, remember } = await waitForSecurityToken();
                return await login(token, remember);
            } finally {
                // Cleared before the awaiting callers run, so a later rejection
                // opens a fresh prompt rather than reusing this settled one.
                authenticationGeneration += 1;
                pendingAuthentication = null;
            }
        })();
    }
    return pendingAuthentication;
}

/**
 * Perform a request, authenticating and retrying for as long as it is rejected.
 * @param {string} url - Absolute URL to request
 * @param {Object} options - fetch options, merged over the credentialed defaults
 * @returns {Promise<Response>} The successful response
 */
export async function authorizedFetch(url, options = {}) {
    while (true) {
        const generation = authenticationGeneration;

        const response = await fetch(url, {
            credentials: "include",
            ...options,
        });

        if (response.ok) {
            return response;
        }

        if (response.status !== 401) {
            await showErrorModal(
                "Error",
                `Error ${response.status} connecting to server.`
            );
        }

        // A request in flight across an authentication was answered against the
        // credentials of the moment it was sent, so its rejection says nothing
        // about the ones we hold now. Retry it before asking the user again.
        if (authenticationGeneration !== generation) {
            continue;
        }

        await authenticate();
    }
}

/**
 * Request a session from the server, authenticating first if required.
 * @param {string} sessionFromPath - Session name taken from the URL path
 * @param {Object} options - `{ welcome: boolean }`, welcome defaulting to true
 * @returns {Promise<Object>} The full session bootstrap payload
 */
export async function fetchSession(sessionFromPath, options = {}) {
    const baseUrl = getBaseUrl();
    const params = new URLSearchParams();
    if (sessionFromPath) {
        params.set("session", sessionFromPath);
    }
    if (options.welcome === false) {
        params.set("welcome", "false");
    }
    const query = params.toString() ? `?${params.toString()}` : "";

    const response = await authorizedFetch(`${baseUrl}/session${query}`, {
        method: "POST",
        headers: {
            "Content-Type": "application/json",
        },
    });
    return await response.json();
}

/**
 * Fetch the list of live sessions without attaching to any of them.
 * @returns {Promise<Array<Object>>} The session descriptors
 */
export async function fetchSessionList() {
    const baseUrl = getBaseUrl();
    const response = await authorizedFetch(`${baseUrl}/session-list`, {
        method: "GET",
    });
    const payload = await response.json();
    return payload.sessions || [];
}
