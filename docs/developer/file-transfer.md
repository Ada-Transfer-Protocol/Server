# File Transfer

Three packet types move a file through a room. The server routes chunks
like any other traffic — **it never stores files**; every current room
member receives the stream.

## The flow

```
sender                                   room members
  FileInit    {"id","filename","size"}     ─ announce
  FileChunk   [file id 16B][data]  × N     ─ payload pieces (16 KiB data each)
  FileComplete[file id 16B]                ─ done
```

- **FileInit (0x0030)** — JSON metadata. `id` is a sender-chosen UUID;
  `filename` a suggested name (treat as untrusted!); `size` total bytes.
- **FileChunk (0x0031)** — the file's 16-byte id followed by raw data.
  Reference SDKs use 16 384-byte chunks. The id
  prefix lets receivers demultiplex several concurrent transfers.
- **FileComplete (0x0033)** — the 16-byte id, closing the transfer.
- `FileAck (0x0032)` / `FileCancel (0x0034)` are routed for client-side
  flow-control conventions; the reference clients don't emit them.

Delivery inherits room semantics: **at-most-once, current members only** —
someone joining mid-transfer gets a partial stream (detectable because
they never saw `FileInit`, or byte counts won't match `size`).

## Sending

Every SDK has a one-call sender (`sendFile(path)` /
`send_file(path)` / browser `sendFile(File)`) — see the SDK pages.
Senders pace chunks slightly (a few ms) to be kind to slow receivers;
the per-connection outbound queue drops on overflow
([reliability](../architecture/reliability.md)).

## Receiving — assembly pattern

Node sketch (the Python `filetransfer_example.py` and PHP
`filetransfer_example.php` implement the same shape):

```js
const fs = require('fs');
const { MessageType } = require('adatp');
const active = new Map();   // idHex → { fd, name, written, size }

for (;;) {
    const pkt = await client.readNextPacket();
    const body = client.decryptIfNeeded ? pkt.payload : pkt.payload; // SDK decrypts in helpers
    switch (pkt.header.msgType) {
        case MessageType.FileInit: {
            const meta = JSON.parse(payloadOf(pkt).toString());
            const safe = meta.filename.replace(/[^\w.\-]/g, '_');    // sanitize!
            active.set(meta.id.replace(/-/g, ''), {
                fd: fs.openSync(`downloads/${safe}`, 'w'),
                written: 0, size: meta.size,
            });
            break;
        }
        case MessageType.FileChunk: {
            const p = payloadOf(pkt);
            const id = p.subarray(0, 16).toString('hex');
            const entry = active.get(id);
            if (entry) { fs.writeSync(entry.fd, p.subarray(16)); entry.written += p.length - 16; }
            break;
        }
        case MessageType.FileComplete: {
            const id = payloadOf(pkt).subarray(0, 16).toString('hex');
            const entry = active.get(id);
            if (entry) { fs.closeSync(entry.fd); active.delete(id); }
            break;
        }
    }
}
```

Receiver hygiene:

- **Sanitize `filename`** — never write to a path the sender controls.
- Skip chunks whose id you have no `FileInit` for (mid-join partials).
- Compare bytes written with `size`; mismatch ⇒ incomplete, discard.
- Ignore your own echo (sender session id == yours) unless you want a
  loopback copy.
- All SDKs (including the browser class since v1.0.0) send `filename` in
  the metadata JSON. Receivers that must interoperate with pre-1.0 browser
  senders may additionally accept the legacy `name` key.

## Limits & integrity

- Each chunk packet must fit `MAX_FRAME_BYTES` (default 1 MiB) — the
  16 KiB default is far below it.
- There is no protocol-level checksum in v1; for integrity, hash on the
  sending side and publish the digest in your own metadata (put a
  `"sha256"` field in the `FileInit` JSON — unknown fields are fine), or
  verify out of band.
- Large fan-outs multiply bandwidth by room size; for big files to many
  receivers prefer a dedicated room per transfer.

Server-side policy: a [plugin](../platform/PLUGIN_DEVELOPMENT.md) with the
`file` hook can veto transfers at `FileInit` (size caps, name filters),
and `file.completed` webhooks notify your backend.
