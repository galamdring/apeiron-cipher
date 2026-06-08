// ice.go provides ICE server configuration and TURN credential rotation for
// the Apeiron Cipher signaling server.
//
// # ICE Server Configuration
//
// The signaling server reads ICE server config from environment variables so
// the binary stays config-file-free. Three variables are supported:
//
//   - STUN_SERVERS  — comma-separated STUN URLs, e.g.
//     "stun:stun.l.google.com:19302,stun:stun1.l.google.com:19302"
//   - TURN_SERVERS  — comma-separated TURN URLs, e.g.
//     "turn:turn.example.com:3478"
//   - TURN_SECRET   — shared HMAC-SHA1 secret for short-lived credential
//     generation (RFC 8489 §9.2 / draft-uberti-behave-turn-rest-00).
//
// If TURN_SECRET is empty, TURN servers are still returned in the ICE list
// but without credentials — suitable for TURN servers that allow open access
// (uncommon) or for configuration testing.
//
// # Short-lived TURN Credentials
//
// When TURN_SECRET is set, the server generates time-limited HMAC-SHA1
// credentials for every new session (on "register"). The username is:
//
//	<expiry_unix_timestamp>:<session_id>
//
// The password is:
//
//	base64( HMAC-SHA1( TURN_SECRET, username ) )
//
// The credential expires after TURNCredentialTTL (default 24 h). The TURN
// relay validates these the same way: it knows the shared secret, computes
// the HMAC, and rejects any connection whose timestamp is in the past.
//
// This is the same scheme used by Coturn (--use-auth-secret), Pion's TURN
// server, and Cloudflare Calls TURN — it is broadly interoperable.
package signaling

import (
	"crypto/hmac"
	"crypto/sha1" //nolint:gosec // HMAC-SHA1 is mandated by draft-uberti-behave-turn-rest-00
	"encoding/base64"
	"fmt"
	"os"
	"strings"
	"time"
)

// TURNCredentialTTL is how long issued TURN credentials remain valid.
// 24 hours is the Coturn default and gives plenty of time for a game session.
const TURNCredentialTTL = 24 * time.Hour

// ICEServer is the wire representation of a single ICE server entry.
// It matches the RTCIceServer shape expected by WebRTC implementations.
type ICEServer struct {
	URLs       []string `json:"urls"`
	Username   string   `json:"username,omitempty"`   // only for TURN
	Credential string   `json:"credential,omitempty"` // only for TURN
}

// ICEConfig holds the full ICE server list returned to clients on register.
type ICEConfig struct {
	ICEServers []ICEServer `json:"ice_servers"`
}

// ICEConfigProvider builds ICEConfig values for new sessions. It is
// constructed once at startup and shared across all handler goroutines; all
// fields are immutable after construction so no locking is needed.
type ICEConfigProvider struct {
	stunURLs   []string // parsed from STUN_SERVERS env
	turnURLs   []string // parsed from TURN_SERVERS env
	turnSecret []byte   // parsed from TURN_SECRET env; nil if not set
}

// NewICEConfigProviderFromEnv reads STUN_SERVERS, TURN_SERVERS, and
// TURN_SECRET from the environment and returns a ready-to-use provider.
// It never returns an error — missing variables simply disable the
// corresponding feature.
func NewICEConfigProviderFromEnv() *ICEConfigProvider {
	return &ICEConfigProvider{
		stunURLs:   parseCommaSeparated(os.Getenv("STUN_SERVERS")),
		turnURLs:   parseCommaSeparated(os.Getenv("TURN_SERVERS")),
		turnSecret: parseSecret(os.Getenv("TURN_SECRET")),
	}
}

// NewICEConfigProvider constructs a provider from explicit values.
// Useful for tests and for wiring the hub without depending on env.
func NewICEConfigProvider(stunURLs, turnURLs []string, turnSecret []byte) *ICEConfigProvider {
	return &ICEConfigProvider{
		stunURLs:   stunURLs,
		turnURLs:   turnURLs,
		turnSecret: turnSecret,
	}
}

// IsEmpty reports whether the provider has no ICE servers configured.
// When true the registered payload carries an empty ice_servers list.
func (p *ICEConfigProvider) IsEmpty() bool {
	return len(p.stunURLs) == 0 && len(p.turnURLs) == 0
}

// ConfigForSession returns an ICEConfig for the given session ID. TURN
// credentials are generated fresh for each call (short-lived per-session).
func (p *ICEConfigProvider) ConfigForSession(sessionID string) ICEConfig {
	var servers []ICEServer

	// STUN entries — no credentials needed.
	if len(p.stunURLs) > 0 {
		servers = append(servers, ICEServer{URLs: p.stunURLs})
	}

	// TURN entries — with or without HMAC credentials.
	for _, url := range p.turnURLs {
		entry := ICEServer{URLs: []string{url}}
		if len(p.turnSecret) > 0 {
			username, password := makeTURNCredential(p.turnSecret, sessionID, time.Now().Add(TURNCredentialTTL))
			entry.Username = username
			entry.Credential = password
		}
		servers = append(servers, entry)
	}

	if servers == nil {
		servers = []ICEServer{} // always send an array, never null
	}
	return ICEConfig{ICEServers: servers}
}

// makeTURNCredential generates a short-lived HMAC-SHA1 TURN credential pair.
//
// username = "<expiry_unix>:<sessionID>"
// password = base64( HMAC-SHA1( secret, username ) )
//
// This is the "REST API" scheme documented in draft-uberti-behave-turn-rest-00
// and implemented by Coturn (--use-auth-secret flag).
func makeTURNCredential(secret []byte, sessionID string, expiry time.Time) (username, password string) {
	username = fmt.Sprintf("%d:%s", expiry.Unix(), sessionID)
	mac := hmac.New(sha1.New, secret) //nolint:gosec // HMAC-SHA1, not plain SHA1
	mac.Write([]byte(username))       //nolint:errcheck // hmac.Hash.Write never errors
	password = base64.StdEncoding.EncodeToString(mac.Sum(nil))
	return username, password
}

// parseCommaSeparated splits a comma-delimited env string into a trimmed,
// non-empty string slice. Returns nil when the input is empty.
func parseCommaSeparated(s string) []string {
	if s == "" {
		return nil
	}
	var out []string
	for _, part := range strings.Split(s, ",") {
		part = strings.TrimSpace(part)
		if part != "" {
			out = append(out, part)
		}
	}
	return out
}

// parseSecret converts a non-empty string to a byte slice suitable for HMAC
// use. Returns nil when the input is empty (TURN auth disabled).
func parseSecret(s string) []byte {
	if s == "" {
		return nil
	}
	return []byte(s)
}
