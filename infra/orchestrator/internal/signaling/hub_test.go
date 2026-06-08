// hub_test.go tests the Hub's session and lobby state management using a
// fake Conn that discards sent messages. All tests run in a single goroutine
// against a hub driven by a background context — the hub is the only
// goroutine; we synchronise with it through a small drainHub helper.
package signaling

import (
	"context"
	"encoding/json"
	"testing"
	"time"
)

// fakeConn is a Conn stand-in that records outbound messages and never
// actually opens a TCP connection.
type fakeConn struct {
	sent []Envelope
}

func newFakeConn() *Conn {
	// We bypass Upgrade() by constructing the Conn fields directly.
	// Only the send channel matters for hub tests.
	c := &Conn{
		send:   make(chan []byte, 64),
		closed: make(chan struct{}),
	}
	return c
}

// drainConn reads all buffered messages from c.send into Envelope slice.
func drainConn(c *Conn) []Envelope {
	var envelopes []Envelope
	for {
		select {
		case b := <-c.send:
			var env Envelope
			if err := json.Unmarshal(b, &env); err == nil {
				envelopes = append(envelopes, env)
			}
		default:
			return envelopes
		}
	}
}

// hubWithContext starts a hub in a background goroutine and returns both the
// hub and a cancel func. The hub is drained synchronously after each command
// via a small sleep — good enough for unit tests.
func hubWithContext(t *testing.T) (*Hub, context.CancelFunc) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	h := NewHub()
	go h.Run(ctx)
	return h, cancel
}

// sync gives the hub goroutine time to process queued commands.
func sync() { time.Sleep(10 * time.Millisecond) }

func TestRegister(t *testing.T) {
	h, cancel := hubWithContext(t)
	defer cancel()

	conn := newFakeConn()
	h.Join("s1", conn)
	h.Register("s1", "Alice")
	sync()

	msgs := drainConn(conn)
	if len(msgs) != 1 {
		t.Fatalf("expected 1 message, got %d", len(msgs))
	}
	if msgs[0].Type != MsgRegistered {
		t.Fatalf("expected registered, got %s", msgs[0].Type)
	}
	var p RegisteredPayload
	json.Unmarshal(msgs[0].Payload, &p) //nolint:errcheck
	if p.DisplayName != "Alice" {
		t.Fatalf("expected display name Alice, got %q", p.DisplayName)
	}
}

func TestRegisterDefaultName(t *testing.T) {
	h, cancel := hubWithContext(t)
	defer cancel()

	conn := newFakeConn()
	h.Join("s2s2s2s2", conn)
	h.Register("s2s2s2s2", "") // empty → auto-generated
	sync()

	msgs := drainConn(conn)
	if len(msgs) != 1 {
		t.Fatalf("expected 1 message, got %d", len(msgs))
	}
	var p RegisteredPayload
	json.Unmarshal(msgs[0].Payload, &p) //nolint:errcheck
	if p.DisplayName == "" {
		t.Fatal("expected non-empty auto display name")
	}
}

func TestCreateAndListLobby(t *testing.T) {
	h, cancel := hubWithContext(t)
	defer cancel()

	alice := newFakeConn()
	h.Join("alice", alice)
	h.Register("alice", "Alice")
	sync()
	drainConn(alice) // consume registered ack

	h.CreateLobby("alice", 0)
	sync()

	msgs := drainConn(alice)
	if len(msgs) != 1 || msgs[0].Type != MsgLobbyCreated {
		t.Fatalf("expected lobby_created, got %v", msgs)
	}
	var created LobbyCreatedPayload
	json.Unmarshal(msgs[0].Payload, &created) //nolint:errcheck
	if created.LobbyID == "" {
		t.Fatal("expected non-empty lobby ID")
	}

	// List lobbies from a second session.
	bob := newFakeConn()
	h.Join("bob", bob)
	h.Register("bob", "Bob")
	sync()
	drainConn(bob)

	h.ListLobbies("bob")
	sync()

	bobMsgs := drainConn(bob)
	if len(bobMsgs) != 1 || bobMsgs[0].Type != MsgLobbyList {
		t.Fatalf("expected lobby_list, got %v", bobMsgs)
	}
	var list LobbyListPayload
	json.Unmarshal(bobMsgs[0].Payload, &list) //nolint:errcheck
	if len(list.Lobbies) != 1 {
		t.Fatalf("expected 1 lobby, got %d", len(list.Lobbies))
	}
	if list.Lobbies[0].LobbyID != created.LobbyID {
		t.Fatalf("lobby ID mismatch")
	}
}

func TestJoinLobby_PeerNotifications(t *testing.T) {
	h, cancel := hubWithContext(t)
	defer cancel()

	// Alice creates the lobby.
	alice := newFakeConn()
	h.Join("alice", alice)
	h.Register("alice", "Alice")
	sync()
	drainConn(alice)

	h.CreateLobby("alice", 0)
	sync()
	aliceMsgs := drainConn(alice)
	var created LobbyCreatedPayload
	json.Unmarshal(aliceMsgs[0].Payload, &created) //nolint:errcheck

	// Bob joins.
	bob := newFakeConn()
	h.Join("bob", bob)
	h.Register("bob", "Bob")
	sync()
	drainConn(bob)

	h.JoinLobby("bob", created.LobbyID)
	sync()

	// Bob should receive lobby_joined with Alice as existing peer.
	bobMsgs := drainConn(bob)
	if len(bobMsgs) != 1 || bobMsgs[0].Type != MsgLobbyJoined {
		t.Fatalf("bob: expected lobby_joined, got %v", bobMsgs)
	}
	var joined LobbyJoinedPayload
	json.Unmarshal(bobMsgs[0].Payload, &joined) //nolint:errcheck
	if len(joined.Peers) != 1 || joined.Peers[0].SessionID != "alice" {
		t.Fatalf("bob: expected peer=alice, got %v", joined.Peers)
	}

	// Alice should receive peer_joined for Bob.
	aliceMsgs2 := drainConn(alice)
	if len(aliceMsgs2) != 1 || aliceMsgs2[0].Type != MsgPeerJoined {
		t.Fatalf("alice: expected peer_joined, got %v", aliceMsgs2)
	}
	var pj PeerJoinedPayload
	json.Unmarshal(aliceMsgs2[0].Payload, &pj) //nolint:errcheck
	if pj.Peer.SessionID != "bob" {
		t.Fatalf("alice: expected peer bob, got %v", pj.Peer)
	}
}

func TestRelayOffer(t *testing.T) {
	h, cancel := hubWithContext(t)
	defer cancel()

	alice := newFakeConn()
	bob := newFakeConn()
	h.Join("alice", alice)
	h.Join("bob", bob)
	sync()

	sdp := json.RawMessage(`{"type":"offer","sdp":"v=0 ..."}`)
	h.Relay("alice", "bob", MsgOffer, sdp)
	sync()

	bobMsgs := drainConn(bob)
	if len(bobMsgs) != 1 || bobMsgs[0].Type != MsgRelay {
		t.Fatalf("bob: expected relay, got %v", bobMsgs)
	}
	var delivery RelayDeliveryPayload
	json.Unmarshal(bobMsgs[0].Payload, &delivery) //nolint:errcheck
	if delivery.FromSessionID != "alice" {
		t.Fatalf("expected from=alice, got %s", delivery.FromSessionID)
	}
	if delivery.Kind != MsgOffer {
		t.Fatalf("expected kind=offer, got %s", delivery.Kind)
	}
}

func TestLeaveNotifiesPeers(t *testing.T) {
	h, cancel := hubWithContext(t)
	defer cancel()

	alice := newFakeConn()
	bob := newFakeConn()
	for id, c := range map[string]*Conn{"alice": alice, "bob": bob} {
		h.Join(id, c)
		h.Register(id, id)
	}
	sync()
	drainConn(alice)
	drainConn(bob)

	h.CreateLobby("alice", 0)
	sync()
	aliceMsgs := drainConn(alice)
	var created LobbyCreatedPayload
	json.Unmarshal(aliceMsgs[0].Payload, &created) //nolint:errcheck

	h.JoinLobby("bob", created.LobbyID)
	sync()
	drainConn(alice)
	drainConn(bob)

	// Alice leaves.
	h.LeaveLobby("alice")
	sync()

	// Bob should get peer_left.
	bobMsgs := drainConn(bob)
	if len(bobMsgs) != 1 || bobMsgs[0].Type != MsgPeerLeft {
		t.Fatalf("bob: expected peer_left, got %v", bobMsgs)
	}
	var pl PeerLeftPayload
	json.Unmarshal(bobMsgs[0].Payload, &pl) //nolint:errcheck
	if pl.SessionID != "alice" {
		t.Fatalf("expected left peer=alice, got %s", pl.SessionID)
	}
}

func TestLobbyFullRejectsJoin(t *testing.T) {
	h, cancel := hubWithContext(t)
	defer cancel()

	alice := newFakeConn()
	h.Join("alice", alice)
	h.Register("alice", "Alice")
	sync()
	drainConn(alice)

	// Create lobby with max 1 peer.
	h.CreateLobby("alice", 1)
	sync()
	aliceMsgs := drainConn(alice)
	var created LobbyCreatedPayload
	json.Unmarshal(aliceMsgs[0].Payload, &created) //nolint:errcheck

	// Bob tries to join but it's full.
	bob := newFakeConn()
	h.Join("bob", bob)
	h.Register("bob", "Bob")
	sync()
	drainConn(bob)

	h.JoinLobby("bob", created.LobbyID)
	sync()

	bobMsgs := drainConn(bob)
	if len(bobMsgs) != 1 || bobMsgs[0].Type != MsgError {
		t.Fatalf("bob: expected error, got %v", bobMsgs)
	}
	var ep ErrorPayload
	json.Unmarshal(bobMsgs[0].Payload, &ep) //nolint:errcheck
	if ep.Code != "lobby_full" {
		t.Fatalf("expected lobby_full, got %s", ep.Code)
	}
}

func TestDisconnectCleansLobby(t *testing.T) {
	h, cancel := hubWithContext(t)
	defer cancel()

	alice := newFakeConn()
	bob := newFakeConn()
	for id, c := range map[string]*Conn{"alice": alice, "bob": bob} {
		h.Join(id, c)
		h.Register(id, id)
	}
	sync()
	drainConn(alice)
	drainConn(bob)

	h.CreateLobby("alice", 0)
	sync()
	aliceMsgs := drainConn(alice)
	var created LobbyCreatedPayload
	json.Unmarshal(aliceMsgs[0].Payload, &created) //nolint:errcheck

	h.JoinLobby("bob", created.LobbyID)
	sync()
	drainConn(alice)
	drainConn(bob)

	// Alice disconnects.
	h.Leave("alice")
	sync()

	// Bob should receive peer_left.
	bobMsgs := drainConn(bob)
	if len(bobMsgs) != 1 || bobMsgs[0].Type != MsgPeerLeft {
		t.Fatalf("bob: expected peer_left on disconnect, got %v", bobMsgs)
	}
}
