import { initConnectionHandlers } from './connection.js';
import { initAuthentication } from './auth.js';
import { initTerminal } from './terminal.js';
import { setupInputHandlers } from './input.js';
import { initWebSockets } from './websockets.js';

document.addEventListener("DOMContentLoaded", async (event) => {
    initConnectionHandlers();

    const webClientId = await initAuthentication();

    const { term, fitAddon } = initTerminal();
    const sessionName = location.pathname.split("/").pop();

    let sendAnsiKey = (ansiKey) => {
        // This will be replaced by the WebSocket module
    };
    
    setupInputHandlers(term, sendAnsiKey);

    document.title = sessionName;
    const websockets = initWebSockets(webClientId, sessionName, term, fitAddon, sendAnsiKey);
    
    // Update sendAnsiKey to use the actual WebSocket function returned by initWebSockets
    sendAnsiKey = websockets.sendAnsiKey;
    
    // Update the input handlers with the correct sendAnsiKey function
    setupInputHandlers(term, sendAnsiKey);
});
