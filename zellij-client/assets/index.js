import { initConnectionHandlers } from './connection.js';
import { fetchSession, fetchSessionList } from './auth.js';
import { initTerminal, settleFontSize } from './terminal.js';
import { setupInputHandlers } from './input.js';
import { initWebSockets } from './websockets.js';
import {
    initMobileUi,
    shouldUseStandaloneMenu,
    showStandaloneSessionMenu,
} from './mobile-ui.js';

document.addEventListener("DOMContentLoaded", async (event) => {
    initConnectionHandlers();

    const sessionFromPath = location.pathname.split("/").pop();

    // A session name in the path is an explicit request for that session; only the bare root,
    // which would otherwise boot a welcome session, is replaced by the session menu.
    let welcome = true;
    if (!sessionFromPath && shouldUseStandaloneMenu()) {
        document.title = "Zellij";
        await showStandaloneSessionMenu({ fetchSessions: fetchSessionList });
        welcome = false;
    }

    const boot = await fetchSession(sessionFromPath, { welcome });

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
