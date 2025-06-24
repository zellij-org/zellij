/**
 * Connection-related utility functions and management
 */

import { is_https } from './utils.js';

// Connection state
let reconnectionAttempt = 0;
let isReconnecting = false;
let reconnectionTimeout = null;
let hasConnectedBefore = false;
let isPageUnloading = false;

/**
 * Get the delay for reconnection attempts using exponential backoff
 * @param {number} attempt - The current attempt number (1-based)
 * @returns {number} The delay in seconds
 */
export function getReconnectionDelay(attempt) {
    const delays = [1, 2, 4, 8, 16];
    return delays[Math.min(attempt - 1, delays.length - 1)];
}

/**
 * Check if the server connection is available
 * @returns {Promise<boolean>} true if connection is OK, false otherwise
 */
export async function checkConnection() {
    try {
        let url_prefix = is_https() ? "https" : "http";
        const response = await fetch(`${url_prefix}://${window.location.host}/info/version`, {
            method: 'GET',
            timeout: 5000
        });
        return response.ok;
    } catch (error) {
        return false;
    }
}

/**
 * Handle reconnection attempts with exponential backoff
 * @returns {Promise<void>}
 */
export async function handleReconnection() {
    if (isReconnecting || !hasConnectedBefore || isPageUnloading) {
        return;
    }
    
    isReconnecting = true;
    let currentModal = null;
    
    while (isReconnecting) {
        reconnectionAttempt++;
        const delaySeconds = getReconnectionDelay(reconnectionAttempt);
        
        const result = await showReconnectionModal(reconnectionAttempt, delaySeconds);
        
        if (result.action === 'cancel') {
            if (result.cleanup) result.cleanup();
            isReconnecting = false;
            reconnectionAttempt = 0;
            return;
        }
        
        if (result.action === 'reconnect') {
            currentModal = result.modal;
            const connectionOk = await checkConnection();
            
            if (connectionOk) {
                if (result.cleanup) result.cleanup();
                isReconnecting = false;
                reconnectionAttempt = 0;
                window.location.reload();
                return;
            } else {
                if (result.cleanup) result.cleanup();
                continue;
            }
        }
    }
}

/**
 * Initialize connection handlers and event listeners
 */
export function initConnectionHandlers() {
    window.addEventListener('beforeunload', () => {
        isPageUnloading = true;
    });

    window.addEventListener('pagehide', () => {
        isPageUnloading = true;
    });
}

/**
 * Mark that a connection has been established
 */
export function markConnectionEstablished() {
    hasConnectedBefore = true;
}

/**
 * Reset connection state
 */
export function resetConnectionState() {
    reconnectionAttempt = 0;
    isReconnecting = false;
    reconnectionTimeout = null;
    hasConnectedBefore = false;
    isPageUnloading = false;
}
