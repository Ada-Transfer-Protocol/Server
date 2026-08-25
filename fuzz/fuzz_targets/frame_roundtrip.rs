#![no_main]
//! Encode/decode round-trip invariant for the AdaTP codec.
//!
//! Property under test: any frame the decoder ACCEPTS must survive being
//! re-encoded (`Packet::to_bytes`) and decoded again (`Packet::from_bytes`)
//! to an equivalent header and payload. This surfaces asymmetries between the
//! encoder and decoder that a parse-only fuzzer would miss.
//!
//! Run:
//!   cargo +nightly fuzz run frame_roundtrip

use adatp_core::codec::packet::Packet;
use bytes::Bytes;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(first) = Packet::from_bytes(Bytes::copy_from_slice(data)) {
        // A packet we accepted must re-encode and decode cleanly.
        let reencoded = first.to_bytes();
        let second = Packet::from_bytes(reencoded)
            .expect("a packet that decoded once must decode again after re-encoding");

        assert_eq!(first.header.msg_type, second.header.msg_type);
        assert_eq!(first.header.version, second.header.version);
        assert_eq!(first.header.flags, second.header.flags);
        assert_eq!(first.header.length, second.header.length);
        assert_eq!(first.header.sequence, second.header.sequence);
        assert_eq!(first.header.timestamp, second.header.timestamp);
        assert_eq!(first.header.session_id, second.header.session_id);
        assert_eq!(first.payload, second.payload);
        assert_eq!(first.auth_tag, second.auth_tag);
    }
});
