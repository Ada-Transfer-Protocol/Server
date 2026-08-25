#!/usr/bin/env node
// AdaTP load test — plaintext WebSocket clients hammering text traffic.
//
//   node loadtest.mjs --url ws://127.0.0.1:3000/ws --clients 100 \
//       --rooms 10 --rate 5 --duration 30 \
//       --username user1 --password password123
//
// Each client authenticates, joins one of --rooms rooms, then sends --rate
// text messages per second for --duration seconds. Payloads carry a send
// timestamp, so every received message yields an end-to-end latency sample
// (send → server → broadcast → receive).

import WebSocket from 'ws';

const arg = (name, def) => {
    const i = process.argv.indexOf(`--${name}`);
    return i >= 0 ? process.argv[i + 1] : def;
};
const URL = arg('url', 'ws://127.0.0.1:3000/ws');
const CLIENTS = Number(arg('clients', 50));
const ROOMS = Number(arg('rooms', 5));
const RATE = Number(arg('rate', 5));
const DURATION = Number(arg('duration', 20));
const USERNAME = arg('username', 'user1');
const PASSWORD = arg('password', 'password123');

const MAGIC = 0x41444154, HEADER = 45;
const T = { AuthRequest: 0x0010, AuthSuccess: 0x0013, TextMessage: 0x0020, JoinRoom: 0x00A0, RoomJoined: 0x00A1 };

const encode = (type, payload, sid) => {
    const buf = Buffer.alloc(HEADER + payload.length);
    buf.writeUInt32LE(MAGIC, 0); buf.writeUInt8(1, 4);
    buf.writeUInt32LE(payload.length, 7);
    buf.writeUInt16LE(type, 19);
    sid.copy(buf, 29); payload.copy(buf, HEADER);
    return buf;
};

const stats = {
    connected: 0, connectFailed: 0, authed: 0, joined: 0,
    sent: 0, received: 0, errors: 0, latencies: [],
};

function percentile(sorted, p) {
    if (!sorted.length) return 0;
    return sorted[Math.min(sorted.length - 1, Math.floor(p / 100 * sorted.length))];
}

async function runClient(i) {
    const sid = Buffer.alloc(16);
    sid.writeUInt32LE(i + 1, 0);
    const room = `load-${i % ROOMS}`;

    const ws = await new Promise((resolve, reject) => {
        const s = new WebSocket(URL);
        s.on('open', () => resolve(s));
        s.on('error', reject);
    }).catch(() => null);
    if (!ws) { stats.connectFailed++; return; }
    stats.connected++;

    let authed = false, joined = false;
    ws.on('message', (data) => {
        const buf = Buffer.isBuffer(data) ? data : Buffer.from(data);
        if (buf.length < HEADER || buf.readUInt32LE(0) !== MAGIC) return;
        const type = buf.readUInt16LE(19);
        const len = buf.readUInt32LE(7);
        if (type === T.AuthSuccess && !authed) {
            authed = true; stats.authed++;
            ws.send(encode(T.JoinRoom, Buffer.from(room), sid));
        } else if (type === T.RoomJoined && !joined) {
            joined = true; stats.joined++;
        } else if (type === T.TextMessage) {
            stats.received++;
            const text = buf.subarray(HEADER, HEADER + len).toString();
            const ts = Number(text.split('|')[0]);
            if (Number.isFinite(ts)) stats.latencies.push(Date.now() - ts);
        }
    });
    ws.on('error', () => stats.errors++);

    ws.send(encode(T.AuthRequest, Buffer.from(JSON.stringify({ username: USERNAME, password: PASSWORD })), sid));

    // Sender loop
    const interval = setInterval(() => {
        if (!joined || ws.readyState !== WebSocket.OPEN) return;
        const payload = Buffer.from(`${Date.now()}|c${i}|${'x'.repeat(64)}`);
        ws.send(encode(T.TextMessage, payload, sid));
        stats.sent++;
    }, 1000 / RATE);

    await new Promise(r => setTimeout(r, DURATION * 1000));
    clearInterval(interval);
    try { ws.close(); } catch { /* ignore */ }
}

console.log(`AdaTP load test → ${URL}`);
console.log(`${CLIENTS} clients × ${RATE} msg/s across ${ROOMS} room(s) for ${DURATION}s`);
console.log(`(each message fans out to ~${Math.ceil(CLIENTS / ROOMS)} room members)\n`);

const t0 = Date.now();
await Promise.all(Array.from({ length: CLIENTS }, (_, i) => runClient(i)));
const elapsed = (Date.now() - t0) / 1000;

const lat = stats.latencies.sort((a, b) => a - b);
console.log('--- results ---');
console.log(`connected            ${stats.connected}/${CLIENTS} (failed: ${stats.connectFailed})`);
console.log(`authenticated        ${stats.authed}`);
console.log(`joined rooms         ${stats.joined}`);
console.log(`messages sent        ${stats.sent} (${(stats.sent / elapsed).toFixed(0)}/s)`);
console.log(`messages received    ${stats.received} (${(stats.received / elapsed).toFixed(0)}/s)`);
console.log(`socket errors        ${stats.errors}`);
console.log(`latency p50/p95/p99  ${percentile(lat, 50)}ms / ${percentile(lat, 95)}ms / ${percentile(lat, 99)}ms (n=${lat.length})`);

const ok = stats.connected === CLIENTS && stats.errors === 0 && stats.received > 0;
console.log(ok ? '\nLOAD TEST PASS' : '\nLOAD TEST COMPLETED WITH ISSUES');
process.exit(ok ? 0 : 1);
