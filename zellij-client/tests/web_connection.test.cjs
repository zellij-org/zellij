const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

// Run the connection module with browser globals supplied by each test. The only
// import computes the application URL and is unrelated to lifecycle handling.
const source = fs.readFileSync(path.join(__dirname, '../assets/connection.js'), 'utf8')
    .replace(/^import \{ getBaseUrl \} from "\.\/utils\.js";$/m, '')
    .replace(/^export /gm, '');

function client() {
    const listeners = new Map();
    const calls = { reconnect: 0, reload: 0, disconnected: 0, probe: 0 };
    let finishReconnect;
    const context = vm.createContext({
        window: {
            addEventListener(type, callback) { listeners.set(type, callback); },
            location: { reload() { calls.reload++; } },
        },
        getBaseUrl: () => 'http://localhost',
        fetch: async () => { calls.probe++; return { ok: true }; },
        showErrorModal: async () => { calls.disconnected++; },
        showReconnectionModal: () => {
            calls.reconnect++;
            return new Promise(resolve => { finishReconnect = resolve; });
        },
    });
    vm.runInContext(source, context);
    context.initConnectionHandlers();
    return {
        context,
        calls,
        dispatch(type, persisted = false) { listeners.get(type)?.({ persisted }); },
        async finish(action = 'reconnect') {
            finishReconnect({ action });
            await new Promise(setImmediate);
        },
    };
}

test('initial pageshow does not reconnect', () => {
    const c = client();
    c.dispatch('pageshow');
    assert.equal(c.calls.reconnect, 0);
});

test('a restored page reconnects once when both socket close events arrive', async () => {
    const c = client();
    c.context.markConnectionEstablished();
    c.dispatch('pagehide', true);
    await c.context.handleReconnection();
    await c.context.handleReconnection();
    assert.equal(c.calls.reconnect, 0);
    c.dispatch('pageshow', true);
    // Chromium delivers the close notifications after the persisted pageshow.
    c.context.handleReconnection();
    c.context.handleReconnection();
    assert.equal(c.calls.reconnect, 1);
    await c.finish();
    assert.equal(c.calls.probe, 1);
    assert.equal(c.calls.reload, 1);
});

test('a page that never connected does not reconnect on restoration', async () => {
    const c = client();
    c.dispatch('pagehide', true);
    c.dispatch('pageshow', true);
    await c.context.handleReconnection();
    assert.equal(c.calls.reconnect, 0);
});

test('host kick notifications after restoration do not reconnect', async () => {
    const c = client();
    c.context.markConnectionEstablished();
    c.dispatch('pagehide', true);
    c.dispatch('pageshow', true);
    await Promise.all([
        c.context.handleDisconnected(),
        c.context.handleDisconnected(),
    ]);
    assert.equal(c.calls.disconnected, 1);
    assert.equal(c.calls.reconnect, 0);
    assert.equal(c.calls.reload, 0);
});

test('a host kick remains disconnected after acknowledgment and restoration', async () => {
    const c = client();
    c.context.markConnectionEstablished();
    await c.context.handleDisconnected();
    assert.equal(c.calls.disconnected, 1);
    c.dispatch('pagehide', true);
    c.dispatch('pageshow', true);
    assert.equal(c.calls.reconnect, 0);
    assert.equal(c.calls.reload, 0);
});

test('a host kick received while leaving the page prevents restore recovery', async () => {
    const c = client();
    c.context.markConnectionEstablished();
    c.dispatch('beforeunload');
    c.dispatch('pagehide', true);
    await c.context.handleDisconnected();
    c.dispatch('pageshow', true);
    assert.equal(c.calls.reconnect, 0);
    assert.equal(c.calls.reload, 0);
});

test('a pending reconnection is not duplicated when restored', async () => {
    const c = client();
    c.context.markConnectionEstablished();
    c.context.handleReconnection();
    c.dispatch('pagehide', true);
    c.dispatch('pageshow', true);
    c.context.handleReconnection();
    assert.equal(c.calls.reconnect, 1);
    await c.finish();
    assert.equal(c.calls.reload, 1);
});

test('restore recovery can be cancelled', async () => {
    const c = client();
    c.context.markConnectionEstablished();
    c.dispatch('pagehide', true);
    c.dispatch('pageshow', true);
    c.context.handleReconnection();
    assert.equal(c.calls.reconnect, 1);
    await c.finish('cancel');
    assert.equal(c.calls.probe, 0);
    assert.equal(c.calls.reload, 0);
});

test('ordinary network closure still reconnects', async () => {
    const c = client();
    c.context.markConnectionEstablished();
    c.context.handleReconnection();
    assert.equal(c.calls.reconnect, 1);
    await c.finish();
    assert.equal(c.calls.reload, 1);
});

