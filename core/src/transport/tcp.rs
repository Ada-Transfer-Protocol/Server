use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::codec::packet::{Packet, HEADER_SIZE};
use bytes::BytesMut;
use std::io;

pub struct TcpTransport {
    stream: TcpStream,
    buffer: BytesMut,
}

impl TcpTransport {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buffer: BytesMut::with_capacity(4096),
        }
    }

    pub async fn read_packet(&mut self) -> io::Result<Option<Packet>> {
        // Read header first
        while self.buffer.len() < HEADER_SIZE {
            if self.stream.read_buf(&mut self.buffer).await? == 0 {
                 if self.buffer.is_empty() {
                     return Ok(None); // Clean disconnect
                 }
                 return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Incomplete header"));
            }
        }

        // Parse header to check total length (length field is payload length)
        // Header structure:
        // Magic (4) + Version (1) + Flags (2) + Length (4) ...
        // Length is at offset 4+1+2 = 7 bytes.
        
        let length_slice = &self.buffer[7..11];
        let payload_len = u32::from_le_bytes(length_slice.try_into().unwrap()) as usize;
        
        // We also need to account for AuthTag (16 bytes) if encrypted flag is set.
        // Flags are at offset 4+1=5 (2 bytes).
        let flags_bits = u16::from_le_bytes(self.buffer[5..7].try_into().unwrap());
        // ENCRYPTED flag is 0x0001 (bit 0)
        let is_encrypted = (flags_bits & 0x0001) != 0;
        let extra_len = if is_encrypted { 16 } else { 0 };

        let total_packet_len = HEADER_SIZE + payload_len + extra_len;

        // Read until we have the full packet
        while self.buffer.len() < total_packet_len {
            if self.stream.read_buf(&mut self.buffer).await? == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Incomplete packet"));
            }
        }

        // Extract packet bytes
        let packet_bytes = self.buffer.split_to(total_packet_len).freeze();
        
        Packet::from_bytes(packet_bytes)
            .map(Some)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub async fn write_packet(&mut self, packet: &Packet) -> io::Result<()> {
        let bytes = packet.to_bytes();
        self.stream.write_all(&bytes).await?;
        self.stream.flush().await?;
        Ok(())
    }
}
