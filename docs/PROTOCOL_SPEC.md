# Ada Transfer Protocol (AdaTP) Specification v2.0

## Overview
AdaTP is a lightweight, binary-first protocol designed for real-time applications including Voice, Video, File Transfer, and Chat. It operates over WebSocket (or TCP/QUIC) and uses a custom binary framing for efficiency.

## 1. Framing Structure
Each packet consists of a **Header** (3 bytes) and a **Payload**.

```
[ Type (2 bytes, LE) ] [ Flags (1 byte) ] [ Payload (N bytes) ... ]
```

### Packet Types (Uint16 LE)

| Hex | Name | Payload Description |
| :--- | :--- | :--- |
| `0x0001` | **HandshakeInit** | Client sends "AdaTP v1.0". Server Ack. |
| `0x0010` | **AuthRequest** | JSON: `{ "username": "...", "password": "..." }` |
| `0x0013` | **AuthSuccess** | 16-byte Session ID (SID). |
| `0x0020` | **TextMessage** | UTF-8 String (Chat & Signaling). |
| `0x0030` | **FileInit** | JSON Metadata (Name, Size, Type). |
| `0x0031` | **FileChunk** | Binary Data Chunk. |
| `0x0033` | **FileComplete** | SHA-256 Checksum (Optional). |
| `0x0044` | **VoiceData** | PCM Audio Chunk (16kHz, S16LE, Mono). |
| `0x0053` | **VideoData** | Binary Frame (e.g. H264/VP8 chunk). |
| `0x0060` | **PresenceUpdate**| UTF-8: "JOIN" or "LEAVE" or "BUSY". |
| `0x00A0` | **JoinRoom** | UTF-8 Room Name (e.g. "lobby", "meeting-1"). |

---

## 2. Signaling Standard (Text-Based)

AdaTP uses `TextMessage` (0x0020) for both chat and critical control signaling.

### A. Call Control (Phone Mode)
These messages are routed by the server to specific targets or broadcasted in a signaling room.

- **Invite**: `INVITE:<TargetID>:<RoomID>`
  - Initiate a call. Includes a unique room ID for the media session.
- **Ringing**: `RINGING:<CallerID>`
  - Target notifies Caller that their device is alerting.
- **Accept**: `ACCEPT:<CallerID>:<RoomID>`
  - Target accepts the call and joins the specified media room.
- **Reject**: `REJECT:<CallerID>:<Reason>`
  - Target declines the call.
- **Busy**: `BUSY:<CallerID>`
  - Target is already in another call.
- **Bye**: `BYE:<PeerID>`
  - Terminate the call.

### B. Conference & Discovery (Group Mode)
These messages are broadcasted to everyone in a room to maintain P2P state consistency without server overhead.

- **Discovery Request**: `DISCOVERY:WHO_IS_HERE`
  - Sent by a newcomer upon joining a room.
- **Discovery Response**: `DISCOVERY:I_AM_HERE`
  - Sent by existing peers in response to a request.
- **Graceful Exit**: `DISCOVERY:I_AM_LEAVING`
  - Sent immediately before a user disconnects/leaves.
- **Mute State**: `MUTE:ON` | `MUTE:OFF`
  - Broadcasted when a user toggles their microphone.

### C. System
- **Heartbeat**: `SYS:PING` -> `SYS:PONG` (RTT Measurement)

---

## 3. Audio Specification
- **Codec**: Raw PCM (Pulse Code Modulation)
- **Sample Rate**: 16,000 Hz (16kHz)
- **Bit Depth**: 16-bit Signed Integer (Little Endian)
- **Channels**: Mono (1 Channel)
- **Frame Size**: Typically 2048 or 4096 samples per packet.

> **Note**: This "dumb" audio format allows AdaTP to be extremely lightweight and compatible with AI/ML processing pipelines directly, without decoding Opus/AAC.

---

## 4. Authentication Flow
1. **Connect** WebSocket.
2. Send `AuthRequest` with credentials.
3. Receive `AuthSuccess` with Session ID (SID).
4. Send `JoinRoom` (Default: "lobby").

## 5. Security
- Use WSS (WebSocket Secure) in production.
- Auth is extensible (currently basic Username/Password).
- Room isolation is enforced by the server (packets are only routed to peers in the same room).
