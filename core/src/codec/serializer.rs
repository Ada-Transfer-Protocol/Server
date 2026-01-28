use super::packet::Packet;


pub struct Serializer;

impl Serializer {
    pub fn serialize(packet: &Packet) -> Vec<u8> {
        // Just a wrapper for now, logic is in Packet::to_bytes
        packet.to_bytes().to_vec()
    }
}
