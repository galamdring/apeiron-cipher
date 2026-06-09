// Package signaling implements the WebSocket-based matchmaking and WebRTC
// signaling server. It is intentionally stateless beyond in-memory session
// mapping: no database, no game logic, no persistence. This is the only
// centralized component in the Apeiron Cipher architecture.
//
// Message types and their JSON wire format live here. All messages share
// a top-level "type" discriminator so clients can dispatch on a single
// field without unmarshalling the entire payload.
package signaling

import "encoding/json"

// MsgType is the message-type discriminator present on every wire message.
type MsgType string

const (
	// Client → server
	MsgRegister    MsgType = "register"     // claim an ephemeral session, optionally announce displayName
	MsgCreateLobby MsgType = "create_lobby" // create a new matchmaking lobby
	MsgListLobbies MsgType = "list_lobbies" // request current lobby list
	MsgJoinLobby   MsgType = "join_lobby"   // request to join an existing lobby
	MsgLeave        MsgType = "leave"        // leave current lobby (stay connected)
	MsgOffer        MsgType = "offer"        // WebRTC SDP offer relay (client → peer)
	MsgAnswer       MsgType = "answer"       // WebRTC SDP answer relay (client → peer)
	MsgICE          MsgType = "ice"          // ICE candidate relay (client → peer)
	MsgPing         MsgType = "ping"         // keepalive ping

	// Server → client
	MsgRegistered    MsgType = "registered"     // acknowledge registration, return session ID
	MsgLobbyList     MsgType = "lobby_list"     // current lobby list
	MsgLobbyCreated  MsgType = "lobby_created"  // new lobby acknowledged
	MsgLobbyJoined   MsgType = "lobby_joined"   // join request accepted, includes peer info
	MsgPeerJoined    MsgType = "peer_joined"    // someone else joined the client's lobby
	MsgPeerLeft      MsgType = "peer_left"      // peer disconnected or left
	MsgRelay         MsgType = "relay"          // relayed offer / answer / ice from peer
	MsgError         MsgType = "error"          // error response
	MsgPong          MsgType = "pong"           // keepalive response
)

// Envelope is the outermost wrapper for every message. Payload is kept as
// raw JSON so the hub can inspect Type without a full parse, and so per-type
// handlers can decode only what they need.
type Envelope struct {
	Type    MsgType         `json:"type"`
	Payload json.RawMessage `json:"payload,omitempty"`
}

// --- Registration ---

// RegisterPayload is sent by a client to claim a session.
type RegisterPayload struct {
	DisplayName string `json:"display_name,omitempty"` // optional human label
}

// RegisteredPayload is the server's acknowledgement.
// ICEServers carries the full ICE configuration the client should pass to its
// RTCPeerConnection constructor. This avoids a separate round-trip and means
// the client always uses fresh TURN credentials (short-lived HMAC-signed).
type RegisteredPayload struct {
	SessionID   string      `json:"session_id"`
	DisplayName string      `json:"display_name"`
	ICEServers  []ICEServer `json:"ice_servers"` // may be empty if no ICE servers configured
}

// --- Lobbies ---

// LobbyInfo is the summary view of a lobby included in list responses.
type LobbyInfo struct {
	LobbyID     string `json:"lobby_id"`
	DisplayName string `json:"display_name"`        // lobby creator's display name
	PeerCount   int    `json:"peer_count"`           // including creator
	MaxPeers    int    `json:"max_peers,omitempty"`  // 0 = unlimited
}

// CreateLobbyPayload is sent by a client to create a lobby.
type CreateLobbyPayload struct {
	MaxPeers int `json:"max_peers,omitempty"` // 0 = unlimited
}

// LobbyCreatedPayload is the server's acknowledgement of lobby creation.
type LobbyCreatedPayload struct {
	LobbyID string `json:"lobby_id"`
}

// ListLobbiesPayload (empty — no fields needed for the request)
type ListLobbiesPayload struct{}

// LobbyListPayload is the server's current lobby list response.
type LobbyListPayload struct {
	Lobbies []LobbyInfo `json:"lobbies"`
}

// JoinLobbyPayload is sent by a client requesting to join a lobby.
type JoinLobbyPayload struct {
	LobbyID string `json:"lobby_id"`
}

// LobbyJoinedPayload is sent to the joining client.
type LobbyJoinedPayload struct {
	LobbyID string     `json:"lobby_id"`
	Peers   []PeerInfo `json:"peers"` // existing peers (excluding self)
}

// PeerInfo is a minimal peer descriptor sent inside joined / peer_joined messages.
type PeerInfo struct {
	SessionID   string `json:"session_id"`
	DisplayName string `json:"display_name"`
}

// PeerJoinedPayload is sent to existing lobby members when a new peer joins.
type PeerJoinedPayload struct {
	LobbyID string   `json:"lobby_id"`
	Peer    PeerInfo `json:"peer"`
}

// PeerLeftPayload is sent to remaining lobby members when a peer leaves.
type PeerLeftPayload struct {
	LobbyID   string `json:"lobby_id"`
	SessionID string `json:"session_id"`
}

// --- WebRTC relay ---

// RelayPayload is used for offer, answer, and ICE messages.
// The server routes it to the target peer identified by TargetSessionID.
type RelayPayload struct {
	TargetSessionID string          `json:"target_session_id"`
	Data            json.RawMessage `json:"data"` // opaque SDP or ICE candidate JSON
}

// RelayDeliveryPayload is what the recipient receives — includes sender ID.
type RelayDeliveryPayload struct {
	FromSessionID string          `json:"from_session_id"`
	Kind          MsgType         `json:"kind"` // "offer", "answer", or "ice"
	Data          json.RawMessage `json:"data"`
}

// --- Errors ---

// ErrorPayload carries a machine-readable code and human-readable message.
type ErrorPayload struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

// --- Helpers ---

// MakeEnvelope serialises payload v and wraps it in an Envelope with the
// given type. Panics if v cannot be marshalled — callers should only pass
// known-good structs.
func MakeEnvelope(t MsgType, v any) Envelope {
	b, err := json.Marshal(v)
	if err != nil {
		panic("signaling: MakeEnvelope marshal: " + err.Error())
	}
	return Envelope{Type: t, Payload: b}
}

// MakeError returns an Envelope wrapping an ErrorPayload.
func MakeError(code, message string) Envelope {
	return MakeEnvelope(MsgError, ErrorPayload{Code: code, Message: message})
}
