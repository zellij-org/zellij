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

/**
 * Get the appropriate protocol for the given type
 * @param {string} type - Protocol type: 'http' or 'ws'
 * @returns {string} The appropriate protocol (http/https or ws/wss)
 * @throws {Error} If type is not 'http' or 'ws'
 */
export function getProtocol(type) {
    const isSecure = is_https();

    switch (type) {
        case "http":
            return isSecure ? "https" : "http";
        case "ws":
            return isSecure ? "wss" : "ws";
        default:
            throw new Error(
                `Invalid protocol type: '${type}'. Must be 'http' or 'ws'.`
            );
    }
}

/**
 * Build a base URL for requests including the current path
 * @param {string} protocolType - Protocol type: 'http' or 'ws'
 * @returns {string} Base URL with protocol, host, and current path
 * @throws {Error} If protocolType is not 'http' or 'ws'
 */
export function getBaseUrl(protocolType) {
    const protocol = getProtocol(protocolType);
    const basePath = window.location.pathname.replace(/\/$/, "");
    return `${protocol}://${window.location.host}${basePath}`;
}

export function isMac() {
    if (navigator.userAgentData && navigator.userAgentData.platform) {
        return navigator.userAgentData.platform === "macOS";
    }
    return navigator.platform.toUpperCase().includes("MAC");
}
