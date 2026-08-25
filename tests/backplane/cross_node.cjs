'use strict';
/**
 * Multi-node backplane proof: two AdaTP server nodes share one Redis. A client
 * on node A and a client on node B join the same room; a message sent on A must
 * reach the client on B — which only works if the Redis backplane fans the room
 * broadcast across nodes.
 *
 * Args: <portA> <portB> <server_key_hex>. Uses the Node SDK from ../../../sdks/node.
 * Exit 0 on success (message crossed), 1 otherwise.
 */
const path = require('node:path');
const { AdaTPClient } = require(path.resolve(__dirname, '../../../sdks/node/dist/index.js'));

const portA = parseInt(process.argv[2], 10);
const portB = parseInt(process.argv[3], 10);
const key = process.argv[4];
const ROOM = 'fleet-lobby';
const MSG = 'cross-node hello ' + portA + '->' + portB;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function main() {
  const a = new AdaTPClient('127.0.0.1', portA, { serverKey: key });
  const b = new AdaTPClient('127.0.0.1', portB, { serverKey: key });

  let received = null;
  b.setMessageHandler((sender, text) => { if (text === MSG) received = { sender, text }; });

  await a.connect(); await a.authenticate('alice', ''); await a.joinRoom(ROOM);
  await b.connect(); await b.authenticate('bob', '');   await b.joinRoom(ROOM);
  await sleep(200); // let joins settle across nodes

  await a.sendTextMessage(MSG); // sent on node A only

  for (let i = 0; i < 40 && !received; i++) await sleep(50); // up to ~2s

  await a.disconnect(); await b.disconnect();

  if (received) {
    console.log(`  ok  message sent on node A (:${portA}) was received on node B (:${portB})`);
    console.log('CROSS-NODE BACKPLANE PASSED.');
    process.exit(0);
  } else {
    console.error(`  FAIL  client on node B (:${portB}) never received the message from node A (:${portA})`);
    process.exit(1);
  }
}
main().catch((e) => { console.error('cross-node test error:', e); process.exit(1); });
