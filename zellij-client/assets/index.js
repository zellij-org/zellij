import { initConnectionHandlers } from './connection.js';
import { initAuthentication } from './auth.js';
import { initTerminal } from './terminal.js';
import { setupInputHandlers } from './input.js';
import { initWebSockets } from './websockets.js';
import { initMobileUi } from './mobile-ui.js';

document.addEventListener("DOMContentLoaded", async (event) => {
    initConnectionHandlers();

    const webClientId = await initAuthentication();

    const { term, fitAddon } = initTerminal();
    const sessionName = location.pathname.split("/").pop();

    let sendAnsiKey = (ansiKey) => {};

    setupInputHandlers(term, fitAddon, sendAnsiKey);

    let websockets = null;
    initMobileUi({
        term,
        fitAddon,
        getSendAnsiKey: () => (websockets ? websockets.sendAnsiKey : sendAnsiKey),
    });

    document.title = sessionName;
    websockets = initWebSockets(webClientId, sessionName, term, fitAddon, sendAnsiKey);

    sendAnsiKey = websockets.sendAnsiKey;

    setupInputHandlers(term, fitAddon, sendAnsiKey);
});
