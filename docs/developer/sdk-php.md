# PHP SDK

`sdks/php` — synchronous client on PHP streams (no `ext-sockets`), with
the full protocol: WebSocket transport (`ws://` **and** `wss://`),
automatic X25519 secure session (libsodium), tools, game state, files.

```bash
cd sdks/php && composer install
```

Requirements: PHP ≥ 7.4 with `ext-openssl`, `ext-sodium`, `ext-json`;
`ramsey/uuid`.

## Connecting

```php
require 'vendor/autoload.php';
use AdaTP\Client;

$client = new Client('127.0.0.1', 3000);            // host + port (path /ws)
// new Client('wss://rt.example.com/ws')             // full URL, TLS via ssl:// stream
// new Client('example.com', 3000, '/ws', true)      // host, port, path, secure

$client->connect();          // opens WS, performs the X25519 handshake
```

## Core API

```php
$identity = $client->authenticate('phpbot', 'secret_password');
// → ['user_id' => …, 'username' => …, 'role' => …]   throws on failure

$room = $client->joinRoom('lobby');        // blocks until RoomJoined

$client->sendTextMessage('hi');
$text = $client->readTextMessage();        // next TextMessage (your echo counts)

$client->sendFile('./report.pdf');         // FileInit → 16 KiB chunks → Complete
$client->disconnect();                     // Disconnect packet + close frame
```

## Game state

```php
$client->sendGameState(['v' => 1, 'game' => 'chess', 'state' => $state]);
$state = $client->readGameState();         // array (or raw string)
```

## Tools

```php
$tools = $client->listTools();
// [['name' => …, 'description' => …, 'schema' => …, 'plugin' => …], …]

try {
    $result = $client->callTool('echo.say', ['text' => 'hi', 'uppercase' => true]);
    // → ['echoed' => 'HI', 'caller' => 'phpbot']
} catch (\Exception $e) {
    // message: "Tool 'x' failed: <code>: <message>"
}
```

## Event loops

```php
$stream = $client->getSocket();                    // PHP stream resource
stream_set_blocking(STDIN, false);

while (true) {
    $read = [$stream]; $w = $e = null;
    // Buffered packets are invisible to stream_select — check hasPending().
    if ($client->hasPending() || stream_select($read, $w, $e, 0, 10000) > 0) {
        $pkt = $client->readPacket();
        if ($pkt->header->msgType === \AdaTP\Protocol::MSG_TEXT_MESSAGE) {
            echo '< ' . $client->decryptPacket($pkt) . "\n";
        }
    }
    // … poll STDIN, send with sendTextMessage() …
}
```

`readPacketOfType([Protocol::MSG_…])` waits for specific types and queues
everything else. The bundled `chat-example.php` is a complete terminal
chat built on this loop.

## Laravel

The package ships a service provider and facade
(`AdaTP\Providers\AdaTPServiceProvider`, `AdaTP` alias) with
`config/adatp.php` reading `ADATP_HOST` / `ADATP_PORT` (default 3000) —
auto-discovered via the composer `extra.laravel` block.

## Example programs

`example.php` (connect + send), `chat-example.php` (interactive rooms),
`filetransfer_example.php` (send + receive loop) — run commands in the
[examples index](examples-index.md).
