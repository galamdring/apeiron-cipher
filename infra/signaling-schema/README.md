# @apeiron-cipher/signaling-schema

Shared TypeScript types and JSON Schema for the Apeiron Cipher signaling protocol.

## What this is

The Apeiron Cipher multiplayer architecture relies on a central **signaling server** for
player discovery, lobby management, and WebRTC offer/answer/ICE relay. All payloads
flow over a single WebSocket connection using a top-level `type` discriminator:

```json
{ "type": "register", "payload": { "display_name": "Alice" } }
```

This package provides:

1. **TypeScript types** (`src/index.ts`) — typed `Envelope<T, P>` aliases for every message.
2. **JSON Schema** (`src/schema.ts`) — Draft-07 schema for runtime validation (AJV-compatible).
3. **Example payloads** (`src/examples.ts`) — typed example messages for every type.
4. **Validation helpers** — `validateEnvelope`, `assertEnvelope`.

## Wire format

```
Envelope = { type: MsgType, payload?: <type-specific object> }
```

All types:

| direction        | type            | payload type           | description                          |
|-----------------|-----------------|------------------------|--------------------------------------|
| client → server | `register`      | `RegisterPayload`      | Claim ephemeral session              |
| server → client | `registered`    | `RegisteredPayload`    | Session ID assigned                  |
| client → server | `create_lobby`  | `CreateLobbyPayload`   | Create a matchmaking lobby           |
| server → client | `lobby_created` | `LobbyCreatedPayload`  | Lobby created, id returned           |
| client → server | `list_lobbies`  | _(empty)_              | Request current lobby list           |
| server → client | `lobby_list`    | `LobbyListPayload`     | Current lobbies                      |
| client → server | `join_lobby`    | `JoinLobbyPayload`     | Request to join a lobby              |
| server → client | `lobby_joined`  | `LobbyJoinedPayload`   | Join accepted, peer list returned    |
| server → client | `peer_joined`   | `PeerJoinedPayload`    | Another peer joined the lobby        |
| server → client | `peer_left`     | `PeerLeftPayload`      | A peer left or disconnected          |
| client → server | `leave`         | _(empty)_              | Leave current lobby                  |
| client → server | `offer`         | `RelayPayload`         | Relay WebRTC SDP offer to peer       |
| client → server | `answer`        | `RelayPayload`         | Relay WebRTC SDP answer to peer      |
| client → server | `ice`           | `RelayPayload`         | Relay ICE candidate to peer          |
| server → client | `relay`         | `RelayDeliveryPayload` | Delivered offer/answer/ice from peer |
| server → client | `error`         | `ErrorPayload`         | Error response                       |
| client → server | `ping`          | _(empty)_              | Keepalive                            |
| server → client | `pong`          | _(empty)_              | Keepalive response                   |

## Schema versioning

`SCHEMA_VERSION = "1.0.0"` is exported from both `index.ts` and `schema.ts`.

- **Major bump**: breaking change to the wire format (removed/renamed field or type).
- **Minor bump**: new optional field added to an existing payload.
- **Patch bump**: documentation or tooling only.

Clients should include the schema version in telemetry. Future server versions may
validate `X-Schema-Version` headers to reject incompatible clients.

## Usage

### TypeScript client

```typescript
import { makeEnvelope, parseServerMessage } from "@apeiron-cipher/signaling-schema";

const ws = new WebSocket("wss://signal.apeiron-cipher.internal/ws");

ws.send(JSON.stringify(makeEnvelope("register", { display_name: "Alice" })));

ws.onmessage = (evt) => {
  const env = parseServerMessage(JSON.parse(evt.data));
  if (!env) return;

  switch (env.type) {
    case "registered":
      console.log("Session ID:", env.payload!.session_id);
      break;
    case "lobby_joined":
      console.log("Joined lobby, peers:", env.payload!.peers);
      break;
    case "relay":
      // Incoming WebRTC offer/answer/ice from peer
      handleRelay(env.payload!);
      break;
    case "error":
      console.error(env.payload!.code, env.payload!.message);
      break;
  }
};
```

### Runtime validation

```typescript
import Ajv from "ajv";
import { envelopeSchema, validateEnvelope } from "@apeiron-cipher/signaling-schema/schema";

const raw = JSON.parse(incomingJson);
if (!validateEnvelope(raw)) {
  console.error("Received malformed signaling message, dropping");
  return;
}
```

## Development

```sh
# Install deps
npm install

# Type-check only (no emit)
npm run check

# Build to dist/
npm run build

# Validate all example payloads against the JSON Schema
npm run validate
```

## Relationship to Go server

The Go server implementation lives in:
```
infra/orchestrator/internal/signaling/messages.go
```

This TypeScript package mirrors those types exactly. When the Go types change,
this package must be updated in the same PR and `SCHEMA_VERSION` bumped appropriately.
