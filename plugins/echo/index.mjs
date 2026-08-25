// AdaTP reference plugin: echo.
//
// Protocol: NDJSON over stdio. Each stdin line is a JSON message from the
// server; each stdout line is a JSON message back. Correlated replies must
// carry the incoming "id". See docs/platform/PLUGIN_DEVELOPMENT.md.

import { createInterface } from 'readline';

const send = (msg) => process.stdout.write(JSON.stringify(msg) + '\n');
const log = (message) => send({ op: 'log', level: 'info', message });

const rl = createInterface({ input: process.stdin, terminal: false });

rl.on('line', (line) => {
    let msg;
    try { msg = JSON.parse(line); } catch { return; }

    switch (msg.op) {
        case 'init':
            log(`echo plugin ready (server ${msg.server_version})`);
            break;

        case 'tool_call': {
            if (msg.tool !== 'echo.say') {
                send({ op: 'tool_error', id: msg.id, code: 'tool_not_found', message: `unknown tool ${msg.tool}` });
                break;
            }
            const text = String(msg.args?.text ?? '');
            const result = msg.args?.uppercase ? text.toUpperCase() : text;
            send({ op: 'tool_result', id: msg.id, result: { echoed: result, caller: msg.caller?.username } });
            send({ op: 'emit_event', event: 'echoed', data: { length: result.length } });
            break;
        }

        case 'hook':
            // This plugin registers no hooks; answer veto requests permissively
            // if one ever arrives (defensive).
            if (msg.id && !msg.notify) send({ op: 'hook_result', id: msg.id, allow: true });
            break;

        case 'shutdown':
            log('echo plugin shutting down');
            process.exit(0);
    }
});
