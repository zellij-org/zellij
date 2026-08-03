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
 * Get the application base URL, derived from the location of this module.
 * Modules are always served from `<base>/assets/<file>.js`, so stripping the
 * trailing `/assets/<file>.js` yields the mount point of the web client.
 * @returns {string} Base URL without a trailing slash
 */
export function getBaseUrl() {
    try {
        const moduleUrl = new URL(import.meta.url);
        const path = moduleUrl.pathname.replace(/\/assets\/[^/]*$/, "");
        return `${moduleUrl.origin}${path}`.replace(/\/$/, "");
    } catch (_) {
        return window.location.origin;
    }
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
 * Get the application base URL converted to a WebSocket URL
 * @returns {string} WebSocket base URL
 */
export function getWebSocketBaseUrl() {
    return getBaseUrl().replace(/^https?/, is_https() ? "wss" : "ws");
}
