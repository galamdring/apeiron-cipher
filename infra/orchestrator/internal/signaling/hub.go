// hub.go is the in-memory session hub — the single shared mutable state
// store for the signaling server. Everything here is owned by a single
// goroutine (Hub.Run) and accessed only through the command channel; there
// are no mutexes. This makes concurrency reasoning trivial and keeps the
// critical section fast.
//
// State held:
//   - sessions: sessionID → *Session (registered WebSocket clients)
//   - lobbies:  lobbyID  → *Lobby   (active matchmaking rooms)
//
// A session is created when a client upgrades to WebSocket; it is
// fully registered (visible to other peers) only after the client sends
// a "register" message. Unregistered sessions can still send messages but
// will receive an error until they register.
package signaling

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"log"
)

// session holds per-connection state.
type session struct {
	id          string
	displayName string
	conn        *Conn
	lobbyID     string // empty if not in a lobby
}

// lobby is an in-memory matchmaking room.
type lobby struct {
	id       string
	maxPeers int // 0 = unlimited
	members  []string // sessionIDs, in join order
}

// cmd is a hub command. The hub's command loop is the only goroutine that
// reads or writes sessions/lobbies; callers never touch those maps directly.
type cmd struct {
	kind    cmdKind
	payload any
	reply   chan<- any // nil for fire-and-forget commands
}

type cmdKind int

const (
	cmdJoin         cmdKind = iota // new WebSocket connection
	cmdLeave                       // WebSocket disconnected
	cmdRegister                    // client sent "register"
	cmdCreateLobby                 // client sent "create_lobby"
	cmdListLobbies                 // client sent "list_lobbies"
	cmdJoinLobby                   // client sent "join_lobby"
	cmdLeaveLobby                  // client sent "leave"
	cmdRelay                       // client sent offer/answer/ice
)

// joinPayload is the data for cmdJoin.
type joinPayload struct {
	sessionID string
	conn      *Conn
}

// registerPayload is the data for cmdRegister.
type registerPayload struct {
	sessionID   string
	displayName string
}

// createLobbyPayload is the data for cmdCreateLobby.
type createLobbyPayload struct {
	sessionID string
	maxPeers  int
}

// joinLobbyPayload is the data for cmdJoinLobby.
type joinLobbyPayload struct {
	sessionID string
	lobbyID   string
}

// relayPayload is the data for cmdRelay.
type relayPayload struct {
	fromSessionID   string
	targetSessionID string
	kind            MsgType
	data            []byte // raw JSON
}

// Hub manages all signaling state and message dispatch.
type Hub struct {
	cmds chan cmd
}

// NewHub creates an uninitialised Hub. Call Run to start it.
func NewHub() *Hub {
	return &Hub{cmds: make(chan cmd, 256)}
}

// Run starts the hub's event loop. It returns when ctx is cancelled.
func (h *Hub) Run(ctx context.Context) {
	sessions := make(map[string]*session)
	lobbies := make(map[string]*lobby)

	for {
		select {
		case <-ctx.Done():
			return
		case c := <-h.cmds:
			h.dispatch(c, sessions, lobbies)
		}
	}
}

func (h *Hub) dispatch(c cmd, sessions map[string]*session, lobbies map[string]*lobby) {
	switch c.kind {
	case cmdJoin:
		p := c.payload.(joinPayload)
		sessions[p.sessionID] = &session{id: p.sessionID, conn: p.conn}

	case cmdLeave:
		sessionID := c.payload.(string)
		s, ok := sessions[sessionID]
		if !ok {
			return
		}
		// Remove from lobby first.
		if s.lobbyID != "" {
			h.removePeerFromLobby(s, sessions, lobbies)
		}
		delete(sessions, sessionID)

	case cmdRegister:
		p := c.payload.(registerPayload)
		s, ok := sessions[p.sessionID]
		if !ok {
			return
		}
		name := p.displayName
		if name == "" {
			name = "player-" + p.sessionID[:6]
		}
		s.displayName = name
		s.conn.Send(MakeEnvelope(MsgRegistered, RegisteredPayload{
			SessionID:   s.id,
			DisplayName: s.displayName,
		}))

	case cmdCreateLobby:
		p := c.payload.(createLobbyPayload)
		s, ok := sessions[p.sessionID]
		if !ok {
			return
		}
		if s.displayName == "" {
			s.conn.Send(MakeError("unregistered", "send register before creating a lobby"))
			return
		}
		if s.lobbyID != "" {
			s.conn.Send(MakeError("already_in_lobby", "leave current lobby before creating a new one"))
			return
		}
		id := newID()
		lob := &lobby{id: id, maxPeers: p.maxPeers, members: []string{s.id}}
		lobbies[id] = lob
		s.lobbyID = id
		s.conn.Send(MakeEnvelope(MsgLobbyCreated, LobbyCreatedPayload{LobbyID: id}))

	case cmdListLobbies:
		sessionID := c.payload.(string)
		s, ok := sessions[sessionID]
		if !ok {
			return
		}
		var list []LobbyInfo
		for _, lob := range lobbies {
			var creator string
			if len(lob.members) > 0 {
				if cs, ok := sessions[lob.members[0]]; ok {
					creator = cs.displayName
				}
			}
			list = append(list, LobbyInfo{
				LobbyID:     lob.id,
				DisplayName: creator,
				PeerCount:   len(lob.members),
				MaxPeers:    lob.maxPeers,
			})
		}
		if list == nil {
			list = []LobbyInfo{} // always send an array, never null
		}
		s.conn.Send(MakeEnvelope(MsgLobbyList, LobbyListPayload{Lobbies: list}))

	case cmdJoinLobby:
		p := c.payload.(joinLobbyPayload)
		s, ok := sessions[p.sessionID]
		if !ok {
			return
		}
		if s.displayName == "" {
			s.conn.Send(MakeError("unregistered", "send register before joining a lobby"))
			return
		}
		if s.lobbyID != "" {
			s.conn.Send(MakeError("already_in_lobby", "leave current lobby before joining another"))
			return
		}
		lob, ok := lobbies[p.lobbyID]
		if !ok {
			s.conn.Send(MakeError("lobby_not_found", "lobby does not exist"))
			return
		}
		if lob.maxPeers > 0 && len(lob.members) >= lob.maxPeers {
			s.conn.Send(MakeError("lobby_full", "lobby is at capacity"))
			return
		}

		// Collect existing peers before adding the new one.
		var peers []PeerInfo
		for _, mid := range lob.members {
			if ms, ok := sessions[mid]; ok {
				peers = append(peers, PeerInfo{SessionID: ms.id, DisplayName: ms.displayName})
			}
		}
		if peers == nil {
			peers = []PeerInfo{}
		}

		// Notify existing members of the new peer.
		newPeer := PeerInfo{SessionID: s.id, DisplayName: s.displayName}
		for _, mid := range lob.members {
			if ms, ok := sessions[mid]; ok {
				ms.conn.Send(MakeEnvelope(MsgPeerJoined, PeerJoinedPayload{
					LobbyID: lob.id,
					Peer:    newPeer,
				}))
			}
		}

		// Now add new peer.
		lob.members = append(lob.members, s.id)
		s.lobbyID = lob.id
		s.conn.Send(MakeEnvelope(MsgLobbyJoined, LobbyJoinedPayload{
			LobbyID: lob.id,
			Peers:   peers,
		}))

	case cmdLeaveLobby:
		sessionID := c.payload.(string)
		s, ok := sessions[sessionID]
		if !ok {
			return
		}
		if s.lobbyID == "" {
			s.conn.Send(MakeError("not_in_lobby", "you are not in a lobby"))
			return
		}
		h.removePeerFromLobby(s, sessions, lobbies)

	case cmdRelay:
		p := c.payload.(relayPayload)
		from, ok := sessions[p.fromSessionID]
		if !ok {
			return
		}
		to, ok := sessions[p.targetSessionID]
		if !ok {
			from.conn.Send(MakeError("peer_not_found", "target session does not exist"))
			return
		}
		to.conn.Send(MakeEnvelope(MsgRelay, RelayDeliveryPayload{
			FromSessionID: p.fromSessionID,
			Kind:          p.kind,
			Data:          p.data,
		}))

	default:
		log.Printf("hub: unknown command kind %d", c.kind)
	}
}

// removePeerFromLobby removes s from its lobby, notifies remaining peers, and
// deletes the lobby if it is now empty. Assumes s.lobbyID != "".
func (h *Hub) removePeerFromLobby(s *session, sessions map[string]*session, lobbies map[string]*lobby) {
	lob, ok := lobbies[s.lobbyID]
	if !ok {
		s.lobbyID = ""
		return
	}
	// Remove s from member list.
	newMembers := lob.members[:0]
	for _, mid := range lob.members {
		if mid != s.id {
			newMembers = append(newMembers, mid)
		}
	}
	lob.members = newMembers

	// Notify remaining members.
	for _, mid := range lob.members {
		if ms, ok := sessions[mid]; ok {
			ms.conn.Send(MakeEnvelope(MsgPeerLeft, PeerLeftPayload{
				LobbyID:   lob.id,
				SessionID: s.id,
			}))
		}
	}

	// Reap empty lobby.
	if len(lob.members) == 0 {
		delete(lobbies, lob.id)
	}

	s.lobbyID = ""
}

// --- command senders (called from handler goroutines) ---

func (h *Hub) sendCmd(k cmdKind, payload any) {
	h.cmds <- cmd{kind: k, payload: payload}
}

// Join registers a new WebSocket connection with the hub.
func (h *Hub) Join(sessionID string, conn *Conn) {
	h.sendCmd(cmdJoin, joinPayload{sessionID: sessionID, conn: conn})
}

// Leave removes a session when its WebSocket closes.
func (h *Hub) Leave(sessionID string) {
	h.sendCmd(cmdLeave, sessionID)
}

// Register handles a client "register" message.
func (h *Hub) Register(sessionID, displayName string) {
	h.sendCmd(cmdRegister, registerPayload{sessionID: sessionID, displayName: displayName})
}

// CreateLobby handles a client "create_lobby" message.
func (h *Hub) CreateLobby(sessionID string, maxPeers int) {
	h.sendCmd(cmdCreateLobby, createLobbyPayload{sessionID: sessionID, maxPeers: maxPeers})
}

// ListLobbies handles a client "list_lobbies" message.
func (h *Hub) ListLobbies(sessionID string) {
	h.sendCmd(cmdListLobbies, sessionID)
}

// JoinLobby handles a client "join_lobby" message.
func (h *Hub) JoinLobby(sessionID, lobbyID string) {
	h.sendCmd(cmdJoinLobby, joinLobbyPayload{sessionID: sessionID, lobbyID: lobbyID})
}

// LeaveLobby handles a client "leave" message.
func (h *Hub) LeaveLobby(sessionID string) {
	h.sendCmd(cmdLeaveLobby, sessionID)
}

// Relay routes an offer/answer/ICE message to the target peer.
func (h *Hub) Relay(fromID, toID string, kind MsgType, data []byte) {
	h.sendCmd(cmdRelay, relayPayload{
		fromSessionID:   fromID,
		targetSessionID: toID,
		kind:            kind,
		data:            data,
	})
}

// newID generates a random 8-byte hex session or lobby ID.
func newID() string {
	var b [8]byte
	if _, err := rand.Read(b[:]); err != nil {
		panic("signaling: newID: " + err.Error())
	}
	return hex.EncodeToString(b[:])
}
