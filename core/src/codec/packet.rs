use bytes::{Buf, BufMut, Bytes};
use serde::{Deserialize, Serialize};

use uuid::Uuid;
use bitflags::bitflags;

pub const MAGIC_NUMBER: u32 = 0x41444154; // "ADAT"
pub const HEADER_SIZE: usize = 4 + 1 + 2 + 4 + 8 + 2 + 8 + 16; // 45 bytes

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub struct PacketFlags: u16 {
        const ENCRYPTED  = 0x0001;
        const COMPRESSED = 0x0002;
        const RELIABLE   = 0x0004;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum MessageType {
    // Handshake
    HandshakeInit = 0x0001,
    HandshakeResponse = 0x0002,
    HandshakeComplete = 0x0003,

    // Auth
    AuthRequest = 0x0010,
    AuthChallenge = 0x0011,
    AuthResponse = 0x0012,
    AuthSuccess = 0x0013,
    AuthFailure = 0x0014,

    // Text
    TextMessage = 0x0020,
    TextAck = 0x0021,
    TextRead = 0x0022,

    // File
    FileInit = 0x0030,
    FileChunk = 0x0031,
    FileAck = 0x0032,
    FileComplete = 0x0033,
    FileCancel = 0x0034,

    // Voice
    VoiceInit = 0x0040,
    VoiceOffer = 0x0041,
    VoiceAnswer = 0x0042,
    VoiceIce = 0x0043,
    VoiceData = 0x0044,
    VoiceEnd = 0x0045,

    // Video
    VideoInit = 0x0050,
    VideoOffer = 0x0051,
    VideoAnswer = 0x0052,
    VideoData = 0x0053,
    VideoEnd = 0x0054,

    // Rooms
    JoinRoom = 0x00A0,
    RoomJoined = 0x00A1,

    // Presence
    PresenceUpdate = 0x0060,
    TypingIndicator = 0x0061,

    // System
    Ping = 0x0070,
    Pong = 0x0071,
    Disconnect = 0x00FF,
    
    // Fallback
    Unknown = 0xFFFF,
}

impl From<u16> for MessageType {
    fn from(v: u16) -> Self {
        match v {
             0x0001 => MessageType::HandshakeInit,
             0x0002 => MessageType::HandshakeResponse,
             0x0003 => MessageType::HandshakeComplete,
             0x0010 => MessageType::AuthRequest,
             0x0011 => MessageType::AuthChallenge,
             0x0012 => MessageType::AuthResponse,
             0x0013 => MessageType::AuthSuccess,
             0x0014 => MessageType::AuthFailure,
            0x0020 => MessageType::TextMessage,
             0x0021 => MessageType::TextAck,
             0x0022 => MessageType::TextRead,
             0x0030 => MessageType::FileInit,
             0x0031 => MessageType::FileChunk,
             0x0032 => MessageType::FileAck,
             0x0033 => MessageType::FileComplete,
             0x0034 => MessageType::FileCancel,
             0x0040 => MessageType::VoiceInit,
             0x0041 => MessageType::VoiceOffer,
             0x0042 => MessageType::VoiceAnswer,
             0x0043 => MessageType::VoiceIce,
             0x0044 => MessageType::VoiceData,
             0x0045 => MessageType::VoiceEnd,
             0x0050 => MessageType::VideoInit,
             0x0051 => MessageType::VideoOffer,
             0x0052 => MessageType::VideoAnswer,
             0x0053 => MessageType::VideoData,
             0x0054 => MessageType::VideoEnd,
             0x0060 => MessageType::PresenceUpdate,
             0x0061 => MessageType::TypingIndicator,
             0x0070 => MessageType::Ping,
             0x0071 => MessageType::Pong,
             0x00A0 => MessageType::JoinRoom,
             0x00A1 => MessageType::RoomJoined,
             0x00FF => MessageType::Disconnect,
             _ => MessageType::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PacketHeader {
    pub magic: u32,
    pub version: u8,
    pub flags: PacketFlags,
    pub length: u32,
    pub sequence: u64,
    pub msg_type: MessageType,
    pub timestamp: u64,
    pub session_id: Uuid,
}

impl Default for PacketHeader {
    fn default() -> Self {
        Self {
            magic: MAGIC_NUMBER,
            version: 1,
            flags: PacketFlags::empty(),
            length: 0,
            sequence: 0,
            msg_type: MessageType::Ping,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            session_id: Uuid::nil(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Packet {
    pub header: PacketHeader,
    pub payload: Bytes,
    pub auth_tag: Option<[u8; 16]>,
}

impl Packet {
    pub fn new(msg_type: MessageType, payload: Bytes, session_id: Uuid) -> Self {
        let mut header = PacketHeader::default();
        header.msg_type = msg_type;
        header.length = payload.len() as u32;
        header.session_id = session_id;

        Self {
            header,
            payload,
            auth_tag: None,
        }
    }

    pub fn to_bytes(&self) -> Bytes {
        let mut buf = Vec::with_capacity(HEADER_SIZE + self.payload.len() + if self.auth_tag.is_some() { 16 } else { 0 });
        
        // Write Header
        buf.put_u32_le(self.header.magic);
        buf.put_u8(self.header.version);
        buf.put_u16_le(self.header.flags.bits());
        buf.put_u32_le(self.header.length);
        buf.put_u64_le(self.header.sequence);
        buf.put_u16_le(self.header.msg_type as u16);
        buf.put_u64_le(self.header.timestamp);
        buf.put_slice(self.header.session_id.as_bytes());

        // Write Payload
        buf.put_slice(&self.payload);

        // Write Auth Tag
        if let Some(tag) = self.auth_tag {
            buf.put_slice(&tag);
        }

        Bytes::from(buf)
    }

    pub fn from_bytes(mut data: Bytes) -> Result<Self, &'static str> {
        if data.len() < HEADER_SIZE {
            return Err("Packet too short");
        }

        let magic = data.get_u32_le();
        if magic != MAGIC_NUMBER {
            return Err("Invalid magic number");
        }

        let version = data.get_u8();
        let flags_bits = data.get_u16_le();
        let flags = PacketFlags::from_bits_truncate(flags_bits);
        let length = data.get_u32_le();
        let sequence = data.get_u64_le();
        let msg_type_u16 = data.get_u16_le();
        let msg_type = MessageType::from(msg_type_u16);
        let timestamp = data.get_u64_le();
        
        let mut uuid_bytes = [0u8; 16];
        data.copy_to_slice(&mut uuid_bytes);
        let session_id = Uuid::from_bytes(uuid_bytes);

        // Check if we have enough data for payload
        if (data.len() as u32) < length {
            return Err("Incomplete payload");
        }

        let payload = data.split_to(length as usize);

        let auth_tag = if flags.contains(PacketFlags::ENCRYPTED) {
             if data.len() < 16 {
                 return Err("Missing auth tag");
             }
             let mut tag = [0u8; 16];
             data.copy_to_slice(&mut tag);
             Some(tag)
        } else {
            None
        };

        Ok(Packet {
            header: PacketHeader {
                magic,
                version,
                flags,
                length,
                sequence,
                msg_type,
                timestamp,
                session_id,
            },
            payload,
            auth_tag,
        })
    }
}
