// conn.go provides a thin wrapper around a raw net/http upgraded WebSocket
// connection. Because the standard library does not include a WebSocket
// implementation we perform the opening handshake and framing manually per
// RFC 6455. This keeps the signaling server dependency-free and avoids
// importing gorilla/websocket (which would bring in a CGo-free but still
// sizable dependency).
//
// Design choices:
//   - Text frames only (JSON payloads).
//   - Reads are synchronous (one goroutine per connection calls readLoop).
//   - Writes are serialised through a buffered channel to avoid concurrent
//     writes on the same net.Conn.
//   - Close frames are sent on shutdown; no auto-ping — the hub sends
//     explicit pong responses on MsgPing.
package signaling

import (
	"bufio"
	"crypto/sha1" //nolint:gosec // SHA-1 is mandated by RFC 6455 — not used for security
	"encoding/base64"
	"encoding/binary"
	"encoding/json"
	"errors"
	"io"
	"log"
	"net"
	"net/http"
	"strings"
	"unicode/utf8"
)

const (
	wsGUID          = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
	maxFramePayload = 128 * 1024 // 128 KiB — generous for SDP/ICE, prevents abuse
	sendBuf         = 32         // outbound message channel depth
)

// Conn is a WebSocket connection.
type Conn struct {
	conn     net.Conn
	bufrw    *bufio.ReadWriter
	send     chan []byte // serialised Envelope JSON
	closed   chan struct{}
	closeErr error
}

// Upgrade performs the WebSocket opening handshake. Returns an error if the
// request is not a valid upgrade request or the handshake fails.
func Upgrade(w http.ResponseWriter, r *http.Request) (*Conn, error) {
	if !strings.EqualFold(r.Header.Get("Upgrade"), "websocket") {
		return nil, errors.New("not a websocket upgrade request")
	}
	key := r.Header.Get("Sec-Websocket-Key")
	if key == "" {
		return nil, errors.New("missing Sec-WebSocket-Key")
	}

	// RFC 6455 §4.2.2 — compute accept key
	h := sha1.New() //nolint:gosec // mandated by spec
	h.Write([]byte(key + wsGUID))
	accept := base64.StdEncoding.EncodeToString(h.Sum(nil))

	// Hijack the connection before writing the 101 response so we own it.
	hj, ok := w.(http.Hijacker)
	if !ok {
		return nil, errors.New("response writer does not support hijack")
	}
	netConn, bufrw, err := hj.Hijack()
	if err != nil {
		return nil, err
	}

	// Write the 101 Switching Protocols response directly on the wire.
	resp := "HTTP/1.1 101 Switching Protocols\r\n" +
		"Upgrade: websocket\r\n" +
		"Connection: Upgrade\r\n" +
		"Sec-WebSocket-Accept: " + accept + "\r\n\r\n"
	if _, err := io.WriteString(bufrw, resp); err != nil {
		netConn.Close()
		return nil, err
	}
	if err := bufrw.Flush(); err != nil {
		netConn.Close()
		return nil, err
	}

	c := &Conn{
		conn:   netConn,
		bufrw:  bufrw,
		send:   make(chan []byte, sendBuf),
		closed: make(chan struct{}),
	}
	go c.writeLoop()
	return c, nil
}

// Send queues an envelope for delivery. If the send buffer is full the oldest
// message is dropped to avoid blocking the hub goroutine.
func (c *Conn) Send(env Envelope) {
	b, err := json.Marshal(env)
	if err != nil {
		log.Printf("signaling: Send marshal: %v", err)
		return
	}
	select {
	case c.send <- b:
	case <-c.closed:
	default:
		// Buffer full — drop silently rather than blocking the caller.
		log.Printf("signaling: send buffer full, dropping message type=%s", env.Type)
	}
}

// Close sends a WebSocket close frame and drains the connection.
func (c *Conn) Close() {
	select {
	case <-c.closed:
	default:
		close(c.closed)
		// Best-effort close frame (opcode 0x8, no payload).
		c.writeFrame(0x8, nil) //nolint:errcheck
		c.conn.Close()
	}
}

// Done returns a channel that is closed when the connection is shut down.
func (c *Conn) Done() <-chan struct{} {
	return c.closed
}

// ReadLoop reads frames from the connection and sends decoded Envelopes on
// the returned channel. The channel is closed when the connection ends.
// ReadLoop owns the read path; callers must not read from the raw connection.
func (c *Conn) ReadLoop() <-chan Envelope {
	out := make(chan Envelope, 8)
	go func() {
		defer close(out)
		defer c.Close()
		for {
			env, err := c.readEnvelope()
			if err != nil {
				if !errors.Is(err, net.ErrClosed) && !isEOF(err) {
					log.Printf("signaling: read error: %v", err)
				}
				return
			}
			select {
			case out <- env:
			case <-c.closed:
				return
			}
		}
	}()
	return out
}

// --- internal ---

func (c *Conn) writeLoop() {
	for {
		select {
		case b := <-c.send:
			if err := c.writeFrame(0x1, b); err != nil { // 0x1 = text frame
				log.Printf("signaling: write error: %v", err)
				c.Close()
				return
			}
		case <-c.closed:
			return
		}
	}
}

// writeFrame writes a single WebSocket frame. opcode: 0x1=text, 0x8=close.
// FIN bit is always set (no fragmentation).
func (c *Conn) writeFrame(opcode byte, payload []byte) error {
	n := len(payload)
	header := make([]byte, 2, 10)
	header[0] = 0x80 | opcode // FIN=1 + opcode
	switch {
	case n <= 125:
		header[1] = byte(n)
	case n <= 65535:
		header[1] = 126
		header = append(header, byte(n>>8), byte(n))
	default:
		header[1] = 127
		var b [8]byte
		binary.BigEndian.PutUint64(b[:], uint64(n)) //nolint:gosec
		header = append(header, b[:]...)
	}
	if _, err := c.bufrw.Write(header); err != nil {
		return err
	}
	if n > 0 {
		if _, err := c.bufrw.Write(payload); err != nil {
			return err
		}
	}
	return c.bufrw.Flush()
}

// readEnvelope reads one WebSocket frame and decodes it as an Envelope.
func (c *Conn) readEnvelope() (Envelope, error) {
	payload, err := c.readFrame()
	if err != nil {
		return Envelope{}, err
	}
	var env Envelope
	if err := json.Unmarshal(payload, &env); err != nil {
		return Envelope{}, err
	}
	return env, nil
}

// readFrame reads a single WebSocket frame from the client. RFC 6455 §5:
// client frames are always masked. We unmask and return the payload.
func (c *Conn) readFrame() ([]byte, error) {
	// Read first 2 header bytes.
	h := make([]byte, 2)
	if _, err := io.ReadFull(c.bufrw, h); err != nil {
		return nil, err
	}
	// opcode := h[0] & 0x0f — we don't inspect it for now
	masked := h[1]&0x80 != 0
	n := int(h[1] & 0x7f)

	switch n {
	case 126:
		var ext [2]byte
		if _, err := io.ReadFull(c.bufrw, ext[:]); err != nil {
			return nil, err
		}
		n = int(binary.BigEndian.Uint16(ext[:]))
	case 127:
		var ext [8]byte
		if _, err := io.ReadFull(c.bufrw, ext[:]); err != nil {
			return nil, err
		}
		n = int(binary.BigEndian.Uint64(ext[:]))
	}

	if n > maxFramePayload {
		return nil, errors.New("frame payload exceeds limit")
	}

	var mask [4]byte
	if masked {
		if _, err := io.ReadFull(c.bufrw, mask[:]); err != nil {
			return nil, err
		}
	}

	payload := make([]byte, n)
	if _, err := io.ReadFull(c.bufrw, payload); err != nil {
		return nil, err
	}
	if masked {
		for i := range payload {
			payload[i] ^= mask[i%4]
		}
	}

	// Validate UTF-8 for text frames (opcode 0x1).
	if !utf8.Valid(payload) {
		return nil, errors.New("invalid UTF-8 in text frame")
	}

	return payload, nil
}

func isEOF(err error) bool {
	return errors.Is(err, io.EOF) || errors.Is(err, io.ErrUnexpectedEOF)
}
