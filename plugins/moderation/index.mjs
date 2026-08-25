// AdaTP example plugin: moderation.
//
// Vetoes text messages containing blocked words (hook "text") and exposes
// moderation.check as a callable tool. Blocklist can be overridden with a
// words.json file (["word", ...]) next to this script.

import { createInterface } from 'readline';
import { readFileSync } from 'fs';

let BLOCKLIST = ['badword', 'verybadword'];
try {
    const custom = JSON.parse(readFileSync(new URL('./words.json', import.meta.url), 'utf-8'));
    if (Array.isArray(custom) && custom.length) BLOCKLIST = custom.map(String);
} catch { /* default list */ }

const send = (msg) => process.stdout.write(JSON.stringify(msg) + '\n');
const matches = (text) => {
    const lower = String(text).toLowerCase();
    return BLOCKLIST.filter(w => lower.includes(w.toLowerCase()));
};

const rl = createInterface({ input: process.stdin, terminal: false });

rl.on('line', (line) => {
    let msg;
    try { msg = JSON.parse(line); } catch { return; }

    switch (msg.op) {
        case 'init':
            send({ op: 'log', level: 'info', message: `moderation ready (${BLOCKLIST.length} blocked words)` });
            break;

        case 'tool_call': {
            if (msg.tool !== 'moderation.check') {
                send({ op: 'tool_error', id: msg.id, code: 'tool_not_found', message: `unknown tool ${msg.tool}` });
                break;
            }
            const matched = matches(msg.args?.text ?? '');
            send({ op: 'tool_result', id: msg.id, result: { allowed: matched.length === 0, matched } });
            break;
        }

        case 'hook': {
            if (msg.hook === 'text' && msg.id && !msg.notify) {
                const matched = matches(msg.event?.text ?? '');
                const allow = matched.length === 0;
                send({ op: 'hook_result', id: msg.id, allow });
                if (!allow) {
                    send({ op: 'log', level: 'warn',
                           message: `blocked message from ${msg.event?.sender?.username} in ${msg.event?.room} (${matched.join(',')})` });
                }
            } else if (msg.id && !msg.notify) {
                send({ op: 'hook_result', id: msg.id, allow: true });
            }
            break;
        }

        case 'shutdown':
            process.exit(0);
    }
});
