/**
 * Example payloads for the Apeiron Cipher signaling protocol.
 *
 * These serve as:
 *   1. Human-readable documentation of the wire format.
 *   2. Inputs for the schema validation smoke test.
 *   3. Fixtures for client-side integration tests.
 *
 * All examples are typed using the exported schema types from index.ts.
 */

import type {
  RegisterEnvelope,
  RegisteredEnvelope,
  CreateLobbyEnvelope,
  LobbyCreatedEnvelope,
  ListLobbiesEnvelope,
  LobbyListEnvelope,
  JoinLobbyEnvelope,
  LobbyJoinedEnvelope,
  PeerJoinedEnvelope,
  PeerLeftEnvelope,
  LeaveEnvelope,
  OfferEnvelope,
  AnswerEnvelope,
  IceEnvelope,
  RelayDeliveryEnvelope,
  ErrorEnvelope,
  PingEnvelope,
  PongEnvelope,
} from "./index.js";

// ---------------------------------------------------------------------------
// Registration flow
// ---------------------------------------------------------------------------

export const registerExample: RegisterEnvelope = {
  type: "register",
  payload: { display_name: "Alice" },
};

export const registerNoNameExample: RegisterEnvelope = {
  type: "register",
  // display_name is optional
};

export const registeredExample: RegisteredEnvelope = {
  type: "registered",
  payload: {
    session_id: "7f3a9c21-4b8e-4d1a-9f5c-e3a8b2d60471",
    display_name: "Alice",
  },
};

// ---------------------------------------------------------------------------
// Lobby management flow
// ---------------------------------------------------------------------------

export const createLobbyExample: CreateLobbyEnvelope = {
  type: "create_lobby",
  payload: { max_peers: 4 },
};

export const createLobbyUnlimitedExample: CreateLobbyEnvelope = {
  type: "create_lobby",
  // max_peers absent → unlimited
};

export const lobbyCreatedExample: LobbyCreatedEnvelope = {
  type: "lobby_created",
  payload: { lobby_id: "lby_a1b2c3d4" },
};

export const listLobbiesExample: ListLobbiesEnvelope = {
  type: "list_lobbies",
};

export const lobbyListExample: LobbyListEnvelope = {
  type: "lobby_list",
  payload: {
    lobbies: [
      {
        lobby_id: "lby_a1b2c3d4",
        display_name: "Alice",
        peer_count: 1,
        max_peers: 4,
      },
      {
        lobby_id: "lby_e5f6g7h8",
        display_name: "Bob",
        peer_count: 2,
      },
    ],
  },
};

export const joinLobbyExample: JoinLobbyEnvelope = {
  type: "join_lobby",
  payload: { lobby_id: "lby_a1b2c3d4" },
};

export const lobbyJoinedExample: LobbyJoinedEnvelope = {
  type: "lobby_joined",
  payload: {
    lobby_id: "lby_a1b2c3d4",
    peers: [
      {
        session_id: "7f3a9c21-4b8e-4d1a-9f5c-e3a8b2d60471",
        display_name: "Alice",
      },
    ],
  },
};

export const peerJoinedExample: PeerJoinedEnvelope = {
  type: "peer_joined",
  payload: {
    lobby_id: "lby_a1b2c3d4",
    peer: {
      session_id: "c2d4e6f8-1a3b-5c7d-9e0f-2a4b6c8d0e1f",
      display_name: "Carol",
    },
  },
};

export const peerLeftExample: PeerLeftEnvelope = {
  type: "peer_left",
  payload: {
    lobby_id: "lby_a1b2c3d4",
    session_id: "c2d4e6f8-1a3b-5c7d-9e0f-2a4b6c8d0e1f",
  },
};

export const leaveExample: LeaveEnvelope = {
  type: "leave",
};

// ---------------------------------------------------------------------------
// WebRTC relay flow
// ---------------------------------------------------------------------------

// Simulated SDP offer blob (abbreviated for readability)
const fakeSdpOffer = {
  type: "offer",
  sdp: "v=0\r\no=- 46117317 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n...",
};

export const offerExample: OfferEnvelope = {
  type: "offer",
  payload: {
    target_session_id: "c2d4e6f8-1a3b-5c7d-9e0f-2a4b6c8d0e1f",
    data: fakeSdpOffer,
  },
};

const fakeSdpAnswer = {
  type: "answer",
  sdp: "v=0\r\no=- 46117318 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n...",
};

export const answerExample: AnswerEnvelope = {
  type: "answer",
  payload: {
    target_session_id: "7f3a9c21-4b8e-4d1a-9f5c-e3a8b2d60471",
    data: fakeSdpAnswer,
  },
};

const fakeIceCandidate = {
  candidate: "candidate:1 1 udp 2130706431 192.168.1.100 50000 typ host",
  sdpMid: "0",
  sdpMLineIndex: 0,
};

export const iceExample: IceEnvelope = {
  type: "ice",
  payload: {
    target_session_id: "c2d4e6f8-1a3b-5c7d-9e0f-2a4b6c8d0e1f",
    data: fakeIceCandidate,
  },
};

// What the target receives after the server routes the relay
export const relayDeliveryOfferExample: RelayDeliveryEnvelope = {
  type: "relay",
  payload: {
    from_session_id: "7f3a9c21-4b8e-4d1a-9f5c-e3a8b2d60471",
    kind: "offer",
    data: fakeSdpOffer,
  },
};

export const relayDeliveryIceExample: RelayDeliveryEnvelope = {
  type: "relay",
  payload: {
    from_session_id: "7f3a9c21-4b8e-4d1a-9f5c-e3a8b2d60471",
    kind: "ice",
    data: fakeIceCandidate,
  },
};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

export const errorLobbyNotFoundExample: ErrorEnvelope = {
  type: "error",
  payload: {
    code: "lobby_not_found",
    message: "No lobby with id lby_xxxxxxxx exists",
  },
};

export const errorNotRegisteredExample: ErrorEnvelope = {
  type: "error",
  payload: {
    code: "not_registered",
    message: "You must send a 'register' message before using this endpoint",
  },
};

// ---------------------------------------------------------------------------
// Keepalive
// ---------------------------------------------------------------------------

export const pingExample: PingEnvelope = { type: "ping" };
export const pongExample: PongEnvelope = { type: "pong" };

// ---------------------------------------------------------------------------
// Convenience: all examples as an array (used by validate-examples.ts)
// ---------------------------------------------------------------------------

export const ALL_EXAMPLES = [
  registerExample,
  registerNoNameExample,
  registeredExample,
  createLobbyExample,
  createLobbyUnlimitedExample,
  lobbyCreatedExample,
  listLobbiesExample,
  lobbyListExample,
  joinLobbyExample,
  lobbyJoinedExample,
  peerJoinedExample,
  peerLeftExample,
  leaveExample,
  offerExample,
  answerExample,
  iceExample,
  relayDeliveryOfferExample,
  relayDeliveryIceExample,
  errorLobbyNotFoundExample,
  errorNotRegisteredExample,
  pingExample,
  pongExample,
] as const;
