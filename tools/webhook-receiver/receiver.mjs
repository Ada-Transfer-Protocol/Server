#!/usr/bin/env node
// AdaTP local webhook receiver — development helper.
//
// Prints every delivery and verifies the X-AdaTP-Signature HMAC.
//
//   node receiver.mjs [port] [secret]
//
// Then register it (server started with ADATP_WEBHOOK_ALLOW_PRIVATE=1 for
// localhost delivery):
//
//   curl -X POST http://127.0.0.1:3000/admin/v1/webhooks \
//     -H "x-admin-token: $ADMIN_TOKEN" -H "content-type: application/json" \
//     -d '{"url":"http://127.0.0.1:9099/hook","events":["*"],"secret":"dev-secret"}'

import { createServer } from 'http';
import { createHmac, timingSafeEqual } from 'crypto';

const PORT = Number(process.argv[2] ?? 9099);
const SECRET = process.argv[3] ?? 'dev-secret';

createServer((req, res) => {
    let body = '';
    req.on('data', (c) => body += c);
    req.on('end', () => {
        const signature = req.headers['x-adatp-signature'] ?? '';
        const expected = 'sha256=' + createHmac('sha256', SECRET).update(body).digest('hex');
        let valid = false;
        try {
            valid = signature.length === expected.length &&
                timingSafeEqual(Buffer.from(signature), Buffer.from(expected));
        } catch { valid = false; }

        const event = req.headers['x-adatp-event'] ?? '?';
        const delivery = req.headers['x-adatp-delivery'] ?? '?';
        console.log(`\n━━ ${new Date().toISOString()} ${req.method} ${req.url}`);
        console.log(`   event=${event} delivery=${delivery}`);
        console.log(`   signature ${valid ? 'VALID ✅' : 'INVALID ❌'}`);
        try { console.log('   ' + JSON.stringify(JSON.parse(body), null, 2).replace(/\n/g, '\n   ')); }
        catch { console.log('   (non-JSON body)', body.slice(0, 200)); }

        res.writeHead(valid ? 200 : 401, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ ok: valid }));
    });
}).listen(PORT, () => {
    console.log(`AdaTP webhook receiver listening on http://127.0.0.1:${PORT}`);
    console.log(`Verifying signatures with secret: ${SECRET}`);
});
