Ada Transport Protocol (AdaTP) Specification
Running code: adatp-core v0.1.0

Abstract

   This document specifies the Ada Transport Protocol (AdaTP), a secure,
   binary, message-oriented application layer protocol designed for
   real-time communication, file transfer, and media streaming (VoIP).
   It provides a unified transport mechanism over TCP, UDP, or WebSocket,
   featuring mandatory encryption, multiplexing, and extensibility.

1.  Introduction

   AdaTP is designed to solve the fragmentation of communication protocols
   by providing a single, coherent standard for chat, voice, and file transfer.
   It prioritizes security (mandatory encryption), performance (binary framing),
   and cross-platform compatibility (unified SDKs).

   The key features of AdaTP include:
   
   *  **Zero-Overhead Handshake**: Fast session establishment using ECDH.
   *  **Unified Framing**: All message types (Text, Voice, File) share a common header.
   *  **Transport Agnostic**: Can run over TCP (reliable), UDP (fast), or WebSocket (web).
   *  **End-to-End Encryption**: Built-in support for PFS (Perfect Forward Secrecy).

2.  Packet Structure

   AdaTP uses a fixed-header, variable-payload binary format. All integers
   MUST be encoded in Little-Endian format.

      The AdaTP Header consists of 45 bytes, encoded in Little-Endian:
      
      | Offset | Field      | Type      | Size | Description                      |
      |:------:|:-----------|:----------|:----:|:---------------------------------|
      | 0      | Magic      | uint32_le | 4    | "ADAT" (0x41444154)              |
      | 4      | Version    | uint8     | 1    | Protocol Version (Cur: 1)        |
      | 5      | Flags      | uint16_le | 2    | Bitmask (0x1=Encrypted)          |
      | 7      | Length     | uint32_le | 4    | Payload Length (bytes)           |
      | 11     | Sequence   | uint64_le | 8    | Packet Sequence Number           |
      | 19     | MsgType    | uint16_le | 2    | Message Type ID                  |
      | 21     | Timestamp  | uint64_le | 8    | Unix Timestamp (ms)              |
      | 29     | SessionID  | bytes     | 16   | Connection UUID                  |
      
      **Total Header Size:** 45 Bytes.
      Followed by:
      - **Payload**: Variable length (defined by Length field).
      - **Auth Tag**: 16 bytes (ONLY if Flags has Encrypted bit set).

   2.1.  Field Descriptions

      Magic:  Fixed 4-byte identifiers "ADAT" (0x41 0x44 0x41 0x54).
      
      Version:  Protocol version. Current value is 1 (0x01).
      
      Flags:  Bitmask defining packet attributes.
              Bit 0 (0x0001): ENCRYPTED - Payload is encrypted.
              Bit 1 (0x0002): COMPRESSED - Payload is compressed (GZIP/ZSTD).
              Bit 2 (0x0004): RELIABLE - Hint for UDP reliability layer.
      
      Length:  The size of the Payload in bytes. Max value is 2^32-1 (4GB).
               This length does NOT include the header or auth tag.
      
      Sequence:  Monotonically increasing sequence number. Used for:
                 1. Ordering reliable messages (on UDP).
                 2. Deduplication.
                 3. Nonce generation for encryption (prevents Replay Attacks).
                 
      Message Type:  16-bit identifier for the payload type (see Section 3).

      Reserved: 2-bytes reserved for future use (must be 0x0000).
      
      Timestamp:  Unix timestamp in milliseconds (64-bit). Used for
                  latency calculation and strict window replay protection.
                  
      Session ID:  128-bit UUIDv4. Uniquely identifies the session/connection.
                   Must remain constant for the duration of a session.

3.  Message Registry

   The Message Type field defines the semantic meaning of the payload.

   3.1.  Authentication & Session (0x0000 - 0x000F)
      0x0001  HANDSHAKE_INIT       Client Hello (Pub Key)
      0x0002  HANDSHAKE_RESPONSE   Server Hello (Pub Key + Cert)
      0x0003  HANDSHAKE_COMPLETE   Finished (Encrypted Verify)

   3.2.  Authorization (0x0010 - 0x001F)
      0x0010  AUTH_REQUEST         Login Request (Username/Token)
      0x0011  AUTH_CHALLENGE       Challenge (Salt/Nonce)
      0x0012  AUTH_RESPONSE        Challenge Response
      0x0013  AUTH_SUCCESS         Login OK + User Profile
      0x0014  AUTH_FAILURE         Login Failed + Error Code

   3.3.  Messaging (0x0020 - 0x002F)
      0x0020  TEXT_MESSAGE         Standard UTF-8 Data
      0x0021  TEXT_ACK             Delivery Receipt
      0x0022  TEXT_READ            Read Receipt

   3.4.  File Transfer (0x0030 - 0x003F)
      0x0030  FILE_INIT            Metadata (Name, Size, Hash)
      0x0031  FILE_CHUNK           Binary Data Segment
      0x0032  FILE_ACK             Chunk Confirmation
      0x0033  FILE_COMPLETE        Transfer Finish
      0x0034  FILE_CANCEL          Abort Transfer

   3.5.  Real-Time Media (0x0040 - 0x005F)
      0x0040  VOICE_INIT           Call Setup Request
      0x0041  VOICE_OFFER          WebRTC SDP Offer (JSON)
      0x0042  VOICE_ANSWER         WebRTC SDP Answer (JSON)
      0x0043  VOICE_ICE            ICE Candidate
      0x0044  VOICE_DATA           Raw RTP/Opus Frame (UDP only)
      0x0045  VOICE_END            Call Termination

   3.6.  Presence & System (0x0060 - 0x009F)
      0x0060  PRESENCE_UPDATE      User Status (Online/Offline/Busy)
      0x0061  TYPING_INDICATOR     Typing... Event
      0x0070  PING                 Heartbeat Request
      0x0071  PONG                 Heartbeat Response

   3.7.  Multi-Room (0x00A0 - 0x00AF)
      0x00A0  JOIN_ROOM            Request to switch room (UTF-8 Name)
      0x00A1  ROOM_JOINED          Confirmation / Broadcast of join

   3.8.  Control (0x00F0 - 0x00FF)
      0x00FF  DISCONNECT           Graceful Shutdown

4.  Security & Handshake

   AdaTP mandates a secure handshake before any application data is exchanged.
   The handshake provides Mutual Authentication and Perfect Forward Secrecy.

   4.1.  Cryptographic Primitives
      - Key Exchange: X25519 (Curve25519)
      - Encryption: AES-256-GCM
      - Digital Signatures: Ed25519
      - Hashing: HKDF-SHA256
      
   4.2.  Handshake Finite State Machine
   
      Client State        Message              Server State
      ------------        -------              ------------
      IDLE             --INIT(Cp)-->          WAIT_INIT
                                                 |
                       <--RESP(Sp, Sig)--     NEGOTIATING
                       
      VERIFY_KEYS      --COMPLETE(Enc)-->     VERIFY_CLIENT
      
      ESTABLISHED      <-- (Secure) -->       ESTABLISHED

   4.3.  Derivation of Session Keys
   
      After exchanging ephemeral public keys (Cp, Sp), both parties compute
      the shared secret (Pre-Master Secret, PMS) using ECDH.
      
      Master Secret = HKDF-Extract(Salt=Nonce, IKM=PMS)
      
      Client_Write_Key = HKDF-Expand(Master, "client_write", 32)
      Server_Write_Key = HKDF-Expand(Master, "server_write", 32)
      Client_IV_Root   = HKDF-Expand(Master, "client_iv", 12)
      Server_IV_Root   = HKDF-Expand(Master, "server_iv", 12)

   4.4.  Replay Protection
   
      The `Sequence` (8 bytes) in the Packet Header acts as the implicit nonce
      counterpart. The AES-GCM IV is constructed as:
      
      IV = IV_Root XOR Sequence
      
      The receiver MUST strictly enforce increasing sequence numbers for any
      given session. Packets with repeated or lower sequence numbers MUST
      be dropped and logged as potential replay attacks.

   4.5. Authentication

      After the handshake is established, the client MAY be required to authenticate depending on Server Configuration.

      **Message Types:**
      - `AUTH_REQUEST` (0x0010): Client credentials.
      - `AUTH_SUCCESS` (0x0013): Authentication successful.
      - `AUTH_FAILURE` (0x0014): Authentication failed.

      **Payload Format:**
      - Request: JSON string `{"username": "...", "password": "..."}`
      - Success: JSON string `{"username": "...", "role": "..."}`
      - Failure: Plain text error message.

      **Authentication Flow:**
      1. **Handshake Completion**: Secure channel established.
      2. **Auth Request**: Client sends encrypted `AUTH_REQUEST`.
      3. **Verification**: Server validates credentials against the active driver (File, Database, or API).
      4. **Response**: 
         - If valid, Server sends `AUTH_SUCCESS`.
         - If invalid, Server sends `AUTH_FAILURE` and closes connection.
      
      **Optional vs Mandatory:**
      - If Auth is **Mandatory**, Client MUST send `AUTH_REQUEST` as the first packet after Handshake.
      - If Auth is **Optional**, Client MAY skip `AUTH_REQUEST` and proceed as a Guest (e.g., send `JOIN_ROOM`).
      - If Auth is **Disabled**, `AUTH_REQUEST` might be ignored or rejected.

   4.6.  File Transfer

      AdaTP supports secure, chunked binary file transfer within rooms.
      
      **Message Types:**
      - `FILE_INIT`     (0x0030): Metadata and transfer request.
      - `FILE_CHUNK`    (0x0031): Binary file data payload.
      - `FILE_ACK`      (0x0032): Chunk acknowledgment (Optional).
      - `FILE_COMPLETE` (0x0033): Transfer completion signal.
      - `FILE_CANCEL`   (0x0034): Cancellation signal.
      
      **Protocol Flow:**
      1.  **Initialization**: Sender sends `FILE_INIT`.
          - Payload: JSON `{"id": "UUID", "filename": "...", "size": N}`
          - Server broadcasts this to the room, injecting `"sender": "username"`.
          
      2.  **Chunk Transfer**: Sender splits file into chunks (recommended 16KB).
          - Payload: `[FileID (16 bytes)] [Binary Data]`
          - Packet MUST be encrypted.
          - Server relays strictly to the room members.
          
      3.  **Completion**: Sender sends `FILE_COMPLETE`.
          - Payload: `[FileID (16 bytes)]`
          - Receivers close file handle.

5.  Error Handling

   AdaTP uses specific error codes carried in the `AUTH_FAILURE` or generic
   `ERROR` (0x00EE - Proposed) payloads.

   5.1.  Common Error Codes
      0x01  Protocol Mismatch (Version invalid)
      0x02  Bad Signature (Handshake tampered)
      0x03  Decryption Failed (Wrong key or corrupted tag)
      0x04  Invalid Sequence (Replay detected)
      0x05  Rate Limited (Too many requests)
      0x06  Payload Too Large
      0x07  Internal Server Error

6.  Future Work

   - **Multiplexing**: Support multiple logical channels over a single
     Session ID (e.g., uploading 2 files while chatting).
   - **Compression**: Negotiating compression algorithms (Lz4, Zstd)
     during handshake.
   - **0-RTT Resumption**: Session tickets to resume connections without
     full DH exchange.

---
Copyright (c) 2026 AdaTP Authors. All Rights Reserved.
