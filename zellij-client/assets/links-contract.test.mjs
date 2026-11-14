import test from 'node:test';
import assert from 'node:assert/strict';

import { build_link_handler, isAllowedLinkUri } from './links.js';

test('isAllowedLinkUri allows conservative link schemes', () => {
    for (const uri of [
        'http://example.com',
        'https://example.com/docs?query=1',
        'mailto:test@example.com',
    ]) {
        assert.equal(isAllowedLinkUri(uri), true, uri);
    }
});

test('isAllowedLinkUri rejects unsafe schemes and malformed URLs', () => {
    for (const uri of [
        'javascript:alert(1)',
        'data:text/html,<script>alert(1)</script>',
        'file:///tmp/example.txt',
        'not a URL',
    ]) {
        assert.equal(isAllowedLinkUri(uri), false, uri);
    }
});

test('activateLink opens allowed links with noopener', () => {
    const originalWindow = globalThis.window;
    const calls = [];
    globalThis.window = {
        open(...args) {
            calls.push(args);
        },
    };

    try {
        const { activateLink } = build_link_handler();
        activateLink({}, 'https://example.com/docs');

        assert.deepEqual(calls, [
            ['https://example.com/docs', '_blank', 'noopener'],
        ]);
    } finally {
        if (originalWindow === undefined) {
            delete globalThis.window;
        } else {
            globalThis.window = originalWindow;
        }
    }
});

test('activateLink ignores disallowed and malformed URIs', () => {
    const originalWindow = globalThis.window;
    let calls = 0;
    globalThis.window = {
        open() {
            calls += 1;
        },
    };

    try {
        const { activateLink } = build_link_handler();
        activateLink({}, 'javascript:alert(1)');
        activateLink({}, 'file:///tmp/example.txt');
        activateLink({}, 'not a URL');

        assert.equal(calls, 0);
    } finally {
        if (originalWindow === undefined) {
            delete globalThis.window;
        } else {
            globalThis.window = originalWindow;
        }
    }
});
