// handler.go wires an upgraded WebSocket Conn into the Hub. Each client
// connection runs one goroutine: the readLoop. Outbound messages are queued
// on the Conn's send channel and written by its internal writeLoop.
//
// The handler is responsible for:
//  1. Upgrading the HTTP connection to WebSocket.
//  2. Enforcing the MAX_SESSIONS cap (optional).
//  3. Generating a session ID and joining the hub.
//  4. Dispatching inbound messages to the hub by type.
//  5. Sending pong responses immediately (no hub round-trip needed).
//  6. Calling hub.Leave when the connection closes.
package signaling

import (
	"encoding/json"
	"log"
	"net/http"
)

// HandlerOptions configures optional behaviour for Handler.
type HandlerOptions struct {
	// MaxSessions caps the number of simultaneous WebSocket sessions.
	// Incoming connections are rejected with 503 when the limit is reached.
	// Zero means unlimited (the default).
	MaxSessions int

	// Debug enables per-message logging.
	Debug bool
}

// Handler returns an http.HandlerFunc that upgrades each connection to
// WebSocket and begins the signaling session. Equivalent to
// HandlerWithOptions(hub, HandlerOptions{}).
func Handler(hub *Hub) http.HandlerFunc {
	return HandlerWithOptions(hub, HandlerOptions{})
}

// HandlerWithOptions returns a handler with configurable session limits and
// debug logging.
func HandlerWithOptions(hub *Hub, opts HandlerOptions) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		// Enforce session cap before upgrading — cheaper than upgrading
		// and then immediately closing.
		if opts.MaxSessions > 0 {
			if count := hub.SessionCount(); count >= opts.MaxSessions {
				http.Error(w, "server at capacity", http.StatusServiceUnavailable)
				log.Printf("signaling: connection rejected — at capacity (%d/%d)", count, opts.MaxSessions)
				return
			}
		}

		conn, err := Upgrade(w, r)
		if err != nil {
			http.Error(w, "websocket upgrade failed: "+err.Error(), http.StatusBadRequest)
			return
		}

		sessionID := newID()
		hub.Join(sessionID, conn)
		defer hub.Leave(sessionID)

		log.Printf("signaling: session %s connected from %s", sessionID, r.RemoteAddr)
		defer log.Printf("signaling: session %s disconnected", sessionID)

		for env := range conn.ReadLoop() {
			if opts.Debug {
				log.Printf("signaling: session %s → type=%s", sessionID, env.Type)
			}
			handleMessage(hub, sessionID, conn, env)
		}
	}
}

func handleMessage(hub *Hub, sessionID string, conn *Conn, env Envelope) {
	switch env.Type {
	case MsgRegister:
		var p RegisterPayload
		if err := json.Unmarshal(env.Payload, &p); err != nil {
			conn.Send(MakeError("bad_payload", "register: "+err.Error()))
			return
		}
		hub.Register(sessionID, p.DisplayName)

	case MsgCreateLobby:
		var p CreateLobbyPayload
		if len(env.Payload) > 0 {
			if err := json.Unmarshal(env.Payload, &p); err != nil {
				conn.Send(MakeError("bad_payload", "create_lobby: "+err.Error()))
				return
			}
		}
		hub.CreateLobby(sessionID, p.MaxPeers)

	case MsgListLobbies:
		hub.ListLobbies(sessionID)

	case MsgJoinLobby:
		var p JoinLobbyPayload
		if err := json.Unmarshal(env.Payload, &p); err != nil {
			conn.Send(MakeError("bad_payload", "join_lobby: "+err.Error()))
			return
		}
		if p.LobbyID == "" {
			conn.Send(MakeError("bad_payload", "join_lobby: lobby_id is required"))
			return
		}
		hub.JoinLobby(sessionID, p.LobbyID)

	case MsgLeave:
		hub.LeaveLobby(sessionID)

	case MsgOffer, MsgAnswer, MsgICE:
		var p RelayPayload
		if err := json.Unmarshal(env.Payload, &p); err != nil {
			conn.Send(MakeError("bad_payload", string(env.Type)+": "+err.Error()))
			return
		}
		if p.TargetSessionID == "" {
			conn.Send(MakeError("bad_payload", string(env.Type)+": target_session_id is required"))
			return
		}
		hub.Relay(sessionID, p.TargetSessionID, env.Type, p.Data)

	case MsgPing:
		// Respond directly on the conn — no hub round-trip needed.
		conn.Send(MakeEnvelope(MsgPong, struct{}{}))

	default:
		conn.Send(MakeError("unknown_type", "unknown message type: "+string(env.Type)))
	}
}
