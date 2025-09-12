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
