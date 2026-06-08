/**
 * Apeiron Cipher — Signaling Protocol Schema
 *
 * Canonical TypeScript types and JSON Schema for the WebSocket matchmaking and
 * WebRTC signaling protocol. Mirrors the Go wire types in
 * infra/orchestrator/internal/signaling/messages.go.
 *
 * All messages share a top-level `type` discriminator so clients can dispatch
 * on a single field without deserialising the entire payload.
 *
 * SCHEMA_VERSION must be incremented on any breaking change to the wire format.
 * Non-breaking additions (new optional fields) increment the minor version only.
 */

export const SCHEMA_VERSION = "1.0.0" as const;

// ---------------------------------------------------------------------------
// Message type discriminator
// ---------------------------------------------------------------------------

/** All message types understood by the signaling protocol. */
export type MsgType =
  // Client → server
  | "register"       // Claim an ephemeral session, optionally announce displayName
  | "create_lobby"   // Create a new matchmaking lobby
  | "list_lobbies"   // Request current lobby list
  | "join_lobby"     // Request to join an existing lobby
  | "leave"          // Leave current lobby (stay connected)
  | "offer"          // WebRTC SDP offer relay (client → peer)
  | "answer"         // WebRTC SDP answer relay (client → peer)
  | "ice"            // ICE candidate relay (client → peer)
  | "ping"           // Keepalive ping
  // Server → client
  | "registered"     // Acknowledge registration, return session ID
  | "lobby_list"     // Current lobby list
  | "lobby_created"  // New lobby acknowledged
  | "lobby_joined"   // Join request accepted, includes peer info
  | "peer_joined"    // Someone else joined the client's lobby
  | "peer_left"      // Peer disconnected or left
  | "relay"          // Relayed offer / answer / ice from peer
  | "error"          // Error response
  | "pong";          // Keepalive response

/** Client-to-server message types (outbound from game client). */
export type ClientMsgType = Extract<MsgType,
  | "register"
  | "create_lobby"
  | "list_lobbies"
  | "join_lobby"
  | "leave"
  | "offer"
  | "answer"
  | "ice"
  | "ping"
>;

/** Server-to-client message types (inbound to game client). */
export type ServerMsgType = Extract<MsgType,
  | "registered"
  | "lobby_list"
  | "lobby_created"
  | "lobby_joined"
  | "peer_joined"
  | "peer_left"
  | "relay"
  | "error"
  | "pong"
>;

// ---------------------------------------------------------------------------
// Envelope — top-level wrapper for every message
// ---------------------------------------------------------------------------

/**
 * Every message on the wire is wrapped in an Envelope. The `type` field acts
 * as the discriminator; `payload` contains type-specific JSON.
 *
 * The server keeps payload as raw bytes to allow dispatch-before-decode;
 * clients should decode payload based on the type field.
 */
export interface Envelope<T extends MsgType = MsgType, P = unknown> {
  type: T;
  /** Type-specific payload. Absent (undefined) for payloads with no fields. */
  payload?: P;
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/** Sent by a client to claim an ephemeral session. */
export interface RegisterPayload {
  /** Optional human-readable display name shown to other players. */
  display_name?: string;
}

/** Server acknowledgement of registration. */
export interface RegisteredPayload {
  /** Randomly generated UUID for this session; must be passed as target_session_id in relay messages. */
  session_id: string;
  /** Echo of the display name (server may assign a default if omitted). */
  display_name: string;
}

// ---------------------------------------------------------------------------
// Lobby management
// ---------------------------------------------------------------------------

/** Summary view of a lobby included in list responses. */
export interface LobbyInfo {
  lobby_id: string;
  /** Display name of the lobby creator. */
  display_name: string;
  /** Current peer count including the creator. */
  peer_count: number;
  /** Maximum peers allowed; 0 means unlimited. */
  max_peers?: number;
}

/** Sent by a client to create a new lobby. */
export interface CreateLobbyPayload {
  /** Maximum number of peers; 0 or absent means unlimited. */
  max_peers?: number;
}

/** Server acknowledgement of lobby creation. */
export interface LobbyCreatedPayload {
  lobby_id: string;
}

/** No fields needed for a list request. */
export type ListLobbiesPayload = Record<string, never>;

/** Server response to list_lobbies. */
export interface LobbyListPayload {
  lobbies: LobbyInfo[];
}

/** Sent by a client requesting to join a lobby. */
export interface JoinLobbyPayload {
  lobby_id: string;
}

/** Minimal peer descriptor included in joined/peer_joined messages. */
export interface PeerInfo {
  session_id: string;
  display_name: string;
}

/** Sent to the joining client on successful join. */
export interface LobbyJoinedPayload {
  lobby_id: string;
  /** Existing peers in the lobby, excluding the joiner. */
  peers: PeerInfo[];
}

/** Sent to existing lobby members when a new peer joins. */
export interface PeerJoinedPayload {
  lobby_id: string;
  peer: PeerInfo;
}

/** Sent to remaining lobby members when a peer leaves or disconnects. */
export interface PeerLeftPayload {
  lobby_id: string;
  session_id: string;
}

// ---------------------------------------------------------------------------
// WebRTC relay
// ---------------------------------------------------------------------------

/** Relay types that can travel inside the `kind` field of a relay delivery. */
export type RelayKind = "offer" | "answer" | "ice";

/**
 * Sent by a client to relay SDP offer, SDP answer, or ICE candidate to a peer.
 * The server routes the message to `target_session_id` without inspecting `data`.
 */
export interface RelayPayload {
  target_session_id: string;
  /** Opaque SDP blob or RTCIceCandidate JSON — the server passes this through unchanged. */
  data: unknown;
}

/** What the recipient receives — the same data, enriched with sender identity. */
export interface RelayDeliveryPayload {
  from_session_id: string;
  /** Identifies whether this is an offer, answer, or ICE candidate. */
  kind: RelayKind;
  /** Opaque SDP or ICE data from the sender. */
  data: unknown;
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/** Sent by the server when a request cannot be fulfilled. */
export interface ErrorPayload {
  /** Machine-readable error code (e.g. "not_registered", "lobby_not_found"). */
  code: string;
  /** Human-readable description suitable for logging. */
  message: string;
}

// ---------------------------------------------------------------------------
// Keepalive
// ---------------------------------------------------------------------------

/** Ping has no payload — an empty or absent payload field is expected. */
export type PingPayload = undefined;
/** Pong has no payload. */
export type PongPayload = undefined;

// ---------------------------------------------------------------------------
// Typed Envelope aliases — the recommended way to work with messages
// ---------------------------------------------------------------------------

export type RegisterEnvelope      = Envelope<"register",      RegisterPayload>;
export type CreateLobbyEnvelope   = Envelope<"create_lobby",  CreateLobbyPayload>;
export type ListLobbiesEnvelope   = Envelope<"list_lobbies",  ListLobbiesPayload | undefined>;
export type JoinLobbyEnvelope     = Envelope<"join_lobby",    JoinLobbyPayload>;
export type LeaveEnvelope         = Envelope<"leave",         undefined>;
export type OfferEnvelope         = Envelope<"offer",         RelayPayload>;
export type AnswerEnvelope        = Envelope<"answer",        RelayPayload>;
export type IceEnvelope           = Envelope<"ice",           RelayPayload>;
export type PingEnvelope          = Envelope<"ping",          undefined>;

export type RegisteredEnvelope    = Envelope<"registered",    RegisteredPayload>;
export type LobbyListEnvelope     = Envelope<"lobby_list",    LobbyListPayload>;
export type LobbyCreatedEnvelope  = Envelope<"lobby_created", LobbyCreatedPayload>;
export type LobbyJoinedEnvelope   = Envelope<"lobby_joined",  LobbyJoinedPayload>;
export type PeerJoinedEnvelope    = Envelope<"peer_joined",   PeerJoinedPayload>;
export type PeerLeftEnvelope      = Envelope<"peer_left",     PeerLeftPayload>;
export type RelayDeliveryEnvelope = Envelope<"relay",         RelayDeliveryPayload>;
export type ErrorEnvelope         = Envelope<"error",         ErrorPayload>;
export type PongEnvelope          = Envelope<"pong",          undefined>;

/** Union of all valid client-bound envelopes. */
export type AnyServerEnvelope =
  | RegisteredEnvelope
  | LobbyListEnvelope
  | LobbyCreatedEnvelope
  | LobbyJoinedEnvelope
  | PeerJoinedEnvelope
  | PeerLeftEnvelope
  | RelayDeliveryEnvelope
  | ErrorEnvelope
  | PongEnvelope;

/** Union of all valid server-bound envelopes. */
export type AnyClientEnvelope =
  | RegisterEnvelope
  | CreateLobbyEnvelope
  | ListLobbiesEnvelope
  | JoinLobbyEnvelope
  | LeaveEnvelope
  | OfferEnvelope
  | AnswerEnvelope
  | IceEnvelope
  | PingEnvelope;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Narrow an unknown value to a typed server envelope. Returns the typed
 * envelope if `type` is a known ServerMsgType, or null otherwise.
 *
 * Usage:
 *   const env = parseServerMessage(JSON.parse(rawJson));
 *   if (env?.type === "registered") { ... env.payload.session_id ... }
 */
export function parseServerMessage(raw: unknown): AnyServerEnvelope | null {
  if (
    typeof raw !== "object" ||
    raw === null ||
    !("type" in raw) ||
    typeof (raw as { type: unknown }).type !== "string"
  ) {
    return null;
  }
  const knownServerTypes: ServerMsgType[] = [
    "registered", "lobby_list", "lobby_created", "lobby_joined",
    "peer_joined", "peer_left", "relay", "error", "pong",
  ];
  const t = (raw as { type: string }).type as MsgType;
  if (!(knownServerTypes as string[]).includes(t)) {
    return null;
  }
  return raw as AnyServerEnvelope;
}

/**
 * Build a typed Envelope for sending from client to server.
 *
 * Usage:
 *   const msg = makeEnvelope("register", { display_name: "Alice" });
 *   ws.send(JSON.stringify(msg));
 */
export function makeEnvelope<T extends ClientMsgType, P>(
  type: T,
  payload?: P,
): Envelope<T, P> {
  return payload !== undefined ? { type, payload } : { type };
}
