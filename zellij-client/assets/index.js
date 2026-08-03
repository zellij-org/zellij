import { initConnectionHandlers } from './connection.js';
import { fetchSession } from './auth.js';
import { initTerminal, settleFontSize } from './terminal.js';
import { setupInputHandlers } from './input.js';
import { initWebSockets } from './websockets.js';
import { initMobileUi } from './mobile-ui.js';

document.addEventListener("DOMContentLoaded", async (event) => {
    initConnectionHandlers();

    const boot = await fetchSession(location.pathname.split("/").pop());

    if (!location.pathname.endsWith(`/${boot.session_name}`)) {
        history.replaceState(null, "", boot.session_name);
    }

    const { term, fitAddon } = initTerminal(boot.config);
    settleFontSize(term, fitAddon, boot.config);

    let websockets = null;
    initMobileUi({
        term,
        fitAddon,
        getSendAnsiKey: () => (websockets ? websockets.sendAnsiKey : () => {}),
    });

    document.title = boot.session_name;
    websockets = initWebSockets(boot, term, fitAddon);

    setupInputHandlers(term, fitAddon, websockets.sendAnsiKey);
});
