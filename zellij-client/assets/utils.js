/**
 * Utility functions for the terminal web client
 */

/**
 * Check if the current page is served over HTTPS
 * @returns {boolean} true if protocol is https:, false otherwise
 */
export function is_https() {
    return document.location.protocol === "https:";
}

export function isMac() {
    if (navigator.userAgentData && navigator.userAgentData.platform) {
        return navigator.userAgentData.platform === "macOS";
    }
    return navigator.platform.toUpperCase().includes("MAC");
}

/**
 * Get the base URL from the base href tag
 * @returns {string} Base URL
 */
export function getBaseUrl() {
    const baseElement = document.querySelector("base");
    if (baseElement && baseElement.href) {
        return baseElement.href.replace(/\/$/, ""); // Remove trailing slash
    }
    // Fallback to current origin if no base href
    return window.location.origin;
}

/**
 * Check whether a target URL points at the page already being displayed,
 * ignoring the query string and fragment.
 * @param {string} target absolute or relative URL
 * @returns {boolean} true if navigating there would only reload the page
 */
export function isCurrentLocation(target) {
    try {
        const targetUrl = new URL(target, window.location.href);
        const stripTrailingSlash = (path) => path.replace(/\/$/, "");
        return (
            targetUrl.origin === window.location.origin &&
            stripTrailingSlash(targetUrl.pathname) ===
                stripTrailingSlash(window.location.pathname)
        );
    } catch (_) {
        return false;
    }
}

/**
 * Detect a mobile viewport (coarse pointer + small width, or a mobile UA).
 * @returns {boolean} true if the current viewport is considered mobile
 */
export function isMobileViewport() {
    return (
        (window.matchMedia &&
            window.matchMedia("(pointer: coarse)").matches &&
            window.innerWidth < 600) ||
        /Mobi|Android|iPhone|iPad/i.test(navigator.userAgent)
    );
}

/**
 * Get the base URL from the base href tag and convert to WebSocket URL
 * @returns {string} WebSocket base URL
 */
export function getWebSocketBaseUrl() {
    const baseElement = document.querySelector("base");
    if (baseElement && baseElement.href) {
        const baseUrl = baseElement.href.replace(/\/$/, ""); // Remove trailing slash
        // Convert http/https to ws/wss for WebSocket
        return baseUrl.replace(/^https?/, is_https() ? "wss" : "ws");
    }
    // Fallback to current origin if no base href
    return window.location.origin.replace(/^https?/, is_https() ? "wss" : "ws");
}
