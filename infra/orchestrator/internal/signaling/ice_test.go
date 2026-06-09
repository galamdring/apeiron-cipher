// ice_test.go tests ICE server configuration and TURN credential rotation.
package signaling

import (
	"encoding/base64"
	"encoding/json"
	"strings"
	"testing"
	"time"
	"crypto/hmac"
	"crypto/sha1" //nolint:gosec
)

// --- Unit tests for credential generation ---

func TestMakeTURNCredential_Format(t *testing.T) {
	secret := []byte("supersecret")
	sessionID := "abc123"
	expiry := time.Unix(9999999999, 0) // far future

	username, password := makeTURNCredential(secret, sessionID, expiry)

	// username must be "<expiry>:<sessionID>"
	wantUsername := "9999999999:abc123"
	if username != wantUsername {
		t.Errorf("username: got %q, want %q", username, wantUsername)
	}

	// password must be base64(HMAC-SHA1(secret, username))
	mac := hmac.New(sha1.New, secret) //nolint:gosec
	mac.Write([]byte(username))       //nolint:errcheck
	wantPassword := base64.StdEncoding.EncodeToString(mac.Sum(nil))
	if password != wantPassword {
		t.Errorf("password: got %q, want %q", password, wantPassword)
	}
}

func TestMakeTURNCredential_DifferentSessionsDifferentPasswords(t *testing.T) {
	secret := []byte("sharedkey")
	expiry := time.Now().Add(TURNCredentialTTL)

	_, p1 := makeTURNCredential(secret, "session-aaa", expiry)
	_, p2 := makeTURNCredential(secret, "session-bbb", expiry)

	if p1 == p2 {
		t.Error("different session IDs must produce different credentials")
	}
}

func TestMakeTURNCredential_Expiry(t *testing.T) {
	secret := []byte("sharedkey")

	u1, _ := makeTURNCredential(secret, "s1", time.Now().Add(1*time.Hour))
	u2, _ := makeTURNCredential(secret, "s1", time.Now().Add(24*time.Hour))

	// Timestamps embedded in usernames should differ.
	if u1 == u2 {
		t.Error("different expiry times must produce different usernames")
	}
}

// --- ICEConfigProvider tests ---

func TestICEConfigProvider_Empty(t *testing.T) {
	p := NewICEConfigProvider(nil, nil, nil)
	if !p.IsEmpty() {
		t.Error("provider with no servers should be empty")
	}
	cfg := p.ConfigForSession("s1")
	if len(cfg.ICEServers) != 0 {
		t.Errorf("expected 0 ICE servers, got %d", len(cfg.ICEServers))
	}
}

func TestICEConfigProvider_STUNOnly(t *testing.T) {
	stun := []string{"stun:stun.l.google.com:19302", "stun:stun1.l.google.com:19302"}
	p := NewICEConfigProvider(stun, nil, nil)

	if p.IsEmpty() {
		t.Error("provider with STUN servers should not be empty")
	}
	cfg := p.ConfigForSession("s1")
	if len(cfg.ICEServers) != 1 {
		t.Fatalf("expected 1 ICE server entry (grouped STUN), got %d", len(cfg.ICEServers))
	}
	entry := cfg.ICEServers[0]
	if len(entry.URLs) != 2 {
		t.Errorf("expected 2 STUN URLs, got %d", len(entry.URLs))
	}
	if entry.Username != "" || entry.Credential != "" {
		t.Error("STUN entry must not have credentials")
	}
}

func TestICEConfigProvider_TURNWithSecret(t *testing.T) {
	turn := []string{"turn:turn.example.com:3478"}
	secret := []byte("s3cr3t")
	p := NewICEConfigProvider(nil, turn, secret)

	cfg := p.ConfigForSession("mysession")
	if len(cfg.ICEServers) != 1 {
		t.Fatalf("expected 1 TURN server entry, got %d", len(cfg.ICEServers))
	}
	entry := cfg.ICEServers[0]
	if entry.Username == "" {
		t.Error("TURN entry must have a username")
	}
	if entry.Credential == "" {
		t.Error("TURN entry must have a credential")
	}
	// Username must embed the session ID.
	if !strings.Contains(entry.Username, "mysession") {
		t.Errorf("username %q does not contain session ID", entry.Username)
	}
	// Validate the credential.
	mac := hmac.New(sha1.New, secret) //nolint:gosec
	mac.Write([]byte(entry.Username)) //nolint:errcheck
	wantCred := base64.StdEncoding.EncodeToString(mac.Sum(nil))
	if entry.Credential != wantCred {
		t.Errorf("credential mismatch: got %q, want %q", entry.Credential, wantCred)
	}
}

func TestICEConfigProvider_TURNWithoutSecret(t *testing.T) {
	// TURN servers configured but no secret — server returns URL without creds.
	turn := []string{"turn:open.example.com:3478"}
	p := NewICEConfigProvider(nil, turn, nil)

	cfg := p.ConfigForSession("s1")
	if len(cfg.ICEServers) != 1 {
		t.Fatalf("expected 1 TURN entry, got %d", len(cfg.ICEServers))
	}
	entry := cfg.ICEServers[0]
	if entry.Username != "" || entry.Credential != "" {
		t.Error("TURN entry without secret must not carry credentials")
	}
}

func TestICEConfigProvider_STUNAndTURN(t *testing.T) {
	stun := []string{"stun:stun.l.google.com:19302"}
	turn := []string{"turn:turn.example.com:3478"}
	p := NewICEConfigProvider(stun, turn, []byte("key"))

	cfg := p.ConfigForSession("session42")
	// Expect: 1 entry for all STUN URLs + 1 entry per TURN URL.
	if len(cfg.ICEServers) != 2 {
		t.Fatalf("expected 2 ICE server entries (STUN + TURN), got %d", len(cfg.ICEServers))
	}
	// STUN entry has no credentials.
	if cfg.ICEServers[0].Username != "" {
		t.Error("first entry (STUN) must not have credentials")
	}
	// TURN entry has credentials.
	if cfg.ICEServers[1].Username == "" {
		t.Error("second entry (TURN) must have credentials")
	}
}

// --- Integration: hub returns ice_servers in registered payload ---

func TestRegister_IncludesICEServers(t *testing.T) {
	stun := []string{"stun:stun.l.google.com:19302"}
	turn := []string{"turn:turn.example.com:3478"}
	ice := NewICEConfigProvider(stun, turn, []byte("testkey"))

	h, cancel := hubWithICE(t, ice)
	defer cancel()

	conn := newFakeConn()
	h.Join("s1", conn)
	h.Register("s1", "Alice")
	sync()

	msgs := drainConn(conn)
	if len(msgs) != 1 || msgs[0].Type != MsgRegistered {
		t.Fatalf("expected 1 registered message, got %v", msgs)
	}

	var p RegisteredPayload
	if err := json.Unmarshal(msgs[0].Payload, &p); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if p.DisplayName != "Alice" {
		t.Errorf("expected display name Alice, got %q", p.DisplayName)
	}
	// Two entries: one STUN group + one TURN.
	if len(p.ICEServers) != 2 {
		t.Fatalf("expected 2 ICE server entries in registered payload, got %d: %+v", len(p.ICEServers), p.ICEServers)
	}
	// TURN entry should carry credentials.
	turnEntry := p.ICEServers[1]
	if turnEntry.Username == "" || turnEntry.Credential == "" {
		t.Error("TURN entry in registered payload must carry credentials")
	}
}

func TestRegister_NoICEServersConfigured_EmptyList(t *testing.T) {
	h, cancel := hubWithContext(t) // hubWithContext uses empty ICEConfigProvider
	defer cancel()

	conn := newFakeConn()
	h.Join("s1", conn)
	h.Register("s1", "Alice")
	sync()

	msgs := drainConn(conn)
	if len(msgs) != 1 || msgs[0].Type != MsgRegistered {
		t.Fatalf("expected 1 registered message, got %v", msgs)
	}
	var p RegisteredPayload
	json.Unmarshal(msgs[0].Payload, &p) //nolint:errcheck
	// Must be an array (not null) even when empty.
	if p.ICEServers == nil {
		t.Error("ice_servers must be an array even when empty, not null/omitted")
	}
	if len(p.ICEServers) != 0 {
		t.Errorf("expected empty ICE servers, got %d entries", len(p.ICEServers))
	}
}

func TestTURNCredentials_UniquePerSession(t *testing.T) {
	// Different sessions must get different credentials (prevents credential sharing).
	ice := NewICEConfigProvider(nil, []string{"turn:turn.example.com:3478"}, []byte("key"))

	h, cancel := hubWithICE(t, ice)
	defer cancel()

	c1 := newFakeConn()
	c2 := newFakeConn()
	h.Join("sess-aaa", c1)
	h.Join("sess-bbb", c2)
	h.Register("sess-aaa", "Alice")
	h.Register("sess-bbb", "Bob")
	sync()

	msgs1 := drainConn(c1)
	msgs2 := drainConn(c2)

	var p1, p2 RegisteredPayload
	json.Unmarshal(msgs1[0].Payload, &p1) //nolint:errcheck
	json.Unmarshal(msgs2[0].Payload, &p2) //nolint:errcheck

	if len(p1.ICEServers) == 0 || len(p2.ICEServers) == 0 {
		t.Fatal("both sessions must receive ICE servers")
	}

	// Credentials must differ between sessions.
	if p1.ICEServers[0].Username == p2.ICEServers[0].Username {
		t.Error("different sessions must receive different TURN usernames")
	}
	if p1.ICEServers[0].Credential == p2.ICEServers[0].Credential {
		t.Error("different sessions must receive different TURN credentials")
	}
}

// --- parseCommaSeparated ---

func TestParseCommaSeparated(t *testing.T) {
	cases := []struct {
		input string
		want  []string
	}{
		{"", nil},
		{"stun:a.com:19302", []string{"stun:a.com:19302"}},
		{"stun:a.com, stun:b.com", []string{"stun:a.com", "stun:b.com"}},
		{"  , ", nil}, // only whitespace/commas → nil
	}
	for _, tc := range cases {
		got := parseCommaSeparated(tc.input)
		if len(got) != len(tc.want) {
			t.Errorf("parseCommaSeparated(%q): got %v, want %v", tc.input, got, tc.want)
			continue
		}
		for i := range got {
			if got[i] != tc.want[i] {
				t.Errorf("parseCommaSeparated(%q)[%d]: got %q, want %q", tc.input, i, got[i], tc.want[i])
			}
		}
	}
}
