// Generates the AdaTP golden test vectors (deterministic, no randomness).
// The output JSON is the single source of truth embedded in
// docs/spec/appendix-test-vectors.md and replayed by every SDK's
// conformance runner.
//
// Usage: node generate_vectors.mjs > vectors/adatp-v1-vectors.json

import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const crypto = require('crypto');

const hex = (b) => Buffer.from(b).toString('hex');

// ---- fixed inputs -----------------------------------------------------
const SESSION_ID = Buffer.from('000102030405060708090a0b0c0d0e0f', 'hex');
const TIMESTAMP = 1700000000000n; // fixed
const SHARED_SECRET = Buffer.from(Array.from({ length: 32 }, (_, i) => i)); // 00..1f
const SALT = Buffer.alloc(32);

// ---- framing ----------------------------------------------------------
function encode({ type, flags = 0, seq = 0n, payload, authTag = null }) {
    const buf = Buffer.alloc(45 + payload.length + (authTag ? 16 : 0));
    buf.writeUInt32LE(0x41444154, 0);
    buf.writeUInt8(1, 4);
    buf.writeUInt16LE(flags, 5);
    buf.writeUInt32LE(payload.length, 7);
    buf.writeBigUInt64LE(seq, 11);
    buf.writeUInt16LE(type, 19);
    buf.writeBigUInt64LE(TIMESTAMP, 21);
    SESSION_ID.copy(buf, 29);
    payload.copy(buf, 45);
    if (authTag) authTag.copy(buf, 45 + payload.length);
    return buf;
}

// ---- key derivation ---------------------------------------------------
const kdf = (info, len) =>
    Buffer.from(crypto.hkdfSync('sha256', SHARED_SECRET, SALT, info, len));
const clientWrite = kdf('client_write', 32);
const serverWrite = kdf('server_write', 32);
const clientIv = kdf('client_iv', 12);
const serverIv = kdf('server_iv', 12);

function computeIv(root, seq) {
    const iv = Buffer.from(root);
    const seqBuf = Buffer.alloc(8);
    seqBuf.writeBigUInt64LE(seq);
    for (let i = 0; i < 8; i++) iv[4 + i] ^= seqBuf[i];
    return iv;
}

function gcmEncrypt(key, iv, plaintext) {
    const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);
    const ct = Buffer.concat([cipher.update(plaintext), cipher.final()]);
    return { ciphertext: ct, tag: cipher.getAuthTag() };
}

// ---- vectors ----------------------------------------------------------
const vectors = { name: 'AdaTP v1 golden vectors', version: 1, cases: [] };

// 1. plaintext TextMessage
{
    const payload = Buffer.from('Hello, AdaTP!', 'utf-8');
    vectors.cases.push({
        id: 'frame-plaintext-text',
        description: 'Plaintext TextMessage (0x0020) framing',
        input: {
            msg_type: '0x0020', flags: 0, sequence: '0',
            timestamp_ms: TIMESTAMP.toString(),
            session_id: hex(SESSION_ID),
            payload_utf8: 'Hello, AdaTP!'
        },
        expected_frame_hex: hex(encode({ type: 0x0020, payload })),
    });
}

// 2. plaintext JoinRoom
{
    const payload = Buffer.from('lobby', 'utf-8');
    vectors.cases.push({
        id: 'frame-plaintext-joinroom',
        description: 'Plaintext JoinRoom (0x00A0) framing',
        input: {
            msg_type: '0x00A0', flags: 0, sequence: '0',
            timestamp_ms: TIMESTAMP.toString(),
            session_id: hex(SESSION_ID),
            payload_utf8: 'lobby'
        },
        expected_frame_hex: hex(encode({ type: 0x00A0, payload })),
    });
}

// 3. HKDF key derivation
vectors.cases.push({
    id: 'kdf-hkdf-sha256',
    description: 'HKDF-SHA256 session key derivation (salt = 32 zero bytes)',
    input: { shared_secret_hex: hex(SHARED_SECRET), salt_hex: hex(SALT) },
    expected: {
        client_write_key: hex(clientWrite),
        server_write_key: hex(serverWrite),
        client_iv_root: hex(clientIv),
        server_iv_root: hex(serverIv),
    },
});

// 4. nonce computation
vectors.cases.push({
    id: 'nonce-seq-xor',
    description: 'AES-GCM nonce = iv_root with last 8 bytes XOR seq (LE), seq=5',
    input: { iv_root_hex: hex(clientIv), sequence: '5' },
    expected: { nonce_hex: hex(computeIv(clientIv, 5n)) },
});

// 5. encrypted TextMessage, client->server, seq=1
{
    const plaintext = Buffer.from('secret message', 'utf-8');
    const iv = computeIv(clientIv, 1n);
    const { ciphertext, tag } = gcmEncrypt(clientWrite, iv, plaintext);
    vectors.cases.push({
        id: 'frame-encrypted-text-client',
        description: 'Encrypted TextMessage from client (client_write key, seq=1, no AAD)',
        input: {
            msg_type: '0x0020', flags: '0x0001', sequence: '1',
            timestamp_ms: TIMESTAMP.toString(),
            session_id: hex(SESSION_ID),
            plaintext_utf8: 'secret message',
            shared_secret_hex: hex(SHARED_SECRET),
        },
        expected: {
            nonce_hex: hex(iv),
            ciphertext_hex: hex(ciphertext),
            auth_tag_hex: hex(tag),
            frame_hex: hex(encode({ type: 0x0020, flags: 0x0001, seq: 1n, payload: ciphertext, authTag: tag })),
        },
    });
}

// 6. encrypted GameState, server->client, seq=2
{
    const plaintext = Buffer.from('{"board":[1,0,2],"turn":"p1"}', 'utf-8');
    const iv = computeIv(serverIv, 2n);
    const { ciphertext, tag } = gcmEncrypt(serverWrite, iv, plaintext);
    vectors.cases.push({
        id: 'frame-encrypted-gamestate-server',
        description: 'Encrypted GameState (0x0050) from server (server_write key, seq=2)',
        input: {
            msg_type: '0x0050', flags: '0x0001', sequence: '2',
            timestamp_ms: TIMESTAMP.toString(),
            session_id: hex(SESSION_ID),
            plaintext_utf8: '{"board":[1,0,2],"turn":"p1"}',
            shared_secret_hex: hex(SHARED_SECRET),
        },
        expected: {
            nonce_hex: hex(iv),
            ciphertext_hex: hex(ciphertext),
            auth_tag_hex: hex(tag),
            frame_hex: hex(encode({ type: 0x0050, flags: 0x0001, seq: 2n, payload: ciphertext, authTag: tag })),
        },
    });
}

// 7. negative cases
vectors.cases.push({
    id: 'reject-bad-magic',
    description: 'Frame with wrong magic MUST be rejected',
    input: { frame_hex: 'deadbeef' + hex(encode({ type: 0x0020, payload: Buffer.from('x') })).slice(8) },
    expected: { error: 'invalid_magic' },
});
vectors.cases.push({
    id: 'reject-short-header',
    description: 'Frame shorter than 45 bytes MUST be rejected',
    input: { frame_hex: hex(encode({ type: 0x0020, payload: Buffer.alloc(0) })).slice(0, 60) },
    expected: { error: 'short_header' },
});
vectors.cases.push({
    id: 'reject-tampered-tag',
    description: 'Encrypted frame with flipped tag bit MUST fail decryption',
    input: {
        note: 'Take frame-encrypted-text-client.frame_hex and XOR the last byte with 0x01',
    },
    expected: { error: 'decrypt_failed' },
});

console.log(JSON.stringify(vectors, null, 2));
