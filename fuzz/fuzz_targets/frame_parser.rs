#![no_main]
//! Fuzz the inbound AdaTP frame parser.
//!
//! `Packet::from_bytes` (core/src/codec/packet.rs) is the first thing the
//! server runs on every byte that arrives on a WebSocket connection. A binary
//! parser fed attacker-controlled input must never panic, over-read, or
//! over-allocate — it may only return a decoded `Packet` or an `Err`.
//!
//! Run:
//!   cargo +nightly fuzz run frame_parser

use adatp_core::codec::packet::Packet;
use bytes::Bytes;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Hand the arbitrary bytes to the real decoder. We only require that it
    // terminates without panicking; a well-behaved parser returns Ok/Err.
    let _ = Packet::from_bytes(Bytes::copy_from_slice(data));
});
