/**
 * JSON Schema definitions for the Apeiron Cipher signaling protocol.
 *
 * These schemas mirror the TypeScript types in index.ts and the Go types in
 * infra/orchestrator/internal/signaling/messages.go.
 *
 * Use with AJV (or any JSON Schema Draft-07 validator) to validate incoming
 * or outgoing messages at runtime:
 *
 *   import Ajv from "ajv";
 *   import { envelopeSchema, validateEnvelope } from "./schema";
 *   const valid = validateEnvelope({ type: "register", payload: { display_name: "Alice" } });
 */

export const SCHEMA_VERSION = "1.0.0";

// ---------------------------------------------------------------------------
// Reusable sub-schemas
// ---------------------------------------------------------------------------

export const peerInfoSchema = {
  type: "object",
  required: ["session_id", "display_name"],
  additionalProperties: false,
  properties: {
    session_id:   { type: "string", minLength: 1 },
    display_name: { type: "string" },
  },
} as const;

export const lobbyInfoSchema = {
  type: "object",
  required: ["lobby_id", "display_name", "peer_count"],
  additionalProperties: false,
  properties: {
    lobby_id:     { type: "string", minLength: 1 },
    display_name: { type: "string" },
    peer_count:   { type: "integer", minimum: 0 },
    max_peers:    { type: "integer", minimum: 0 },
  },
} as const;

export const errorPayloadSchema = {
  type: "object",
  required: ["code", "message"],
  additionalProperties: false,
  properties: {
    code:    { type: "string", minLength: 1 },
    message: { type: "string" },
  },
} as const;

// ---------------------------------------------------------------------------
// Per-type payload schemas
// ---------------------------------------------------------------------------

const payloadSchemas: Record<string, object> = {
  register: {
    type: "object",
    additionalProperties: false,
    properties: {
      display_name: { type: "string" },
    },
  },

  registered: {
    type: "object",
    required: ["session_id", "display_name"],
    additionalProperties: false,
    properties: {
      session_id:   { type: "string", minLength: 1 },
      display_name: { type: "string" },
    },
  },

  create_lobby: {
    type: "object",
    additionalProperties: false,
    properties: {
      max_peers: { type: "integer", minimum: 0 },
    },
  },

  lobby_created: {
    type: "object",
    required: ["lobby_id"],
    additionalProperties: false,
    properties: {
      lobby_id: { type: "string", minLength: 1 },
    },
  },

  list_lobbies: {
    type: "object",
    additionalProperties: false,
    properties: {},
  },

  lobby_list: {
    type: "object",
    required: ["lobbies"],
    additionalProperties: false,
    properties: {
      lobbies: {
        type: "array",
        items: lobbyInfoSchema,
      },
    },
  },

  join_lobby: {
    type: "object",
    required: ["lobby_id"],
    additionalProperties: false,
    properties: {
      lobby_id: { type: "string", minLength: 1 },
    },
  },

  lobby_joined: {
    type: "object",
    required: ["lobby_id", "peers"],
    additionalProperties: false,
    properties: {
      lobby_id: { type: "string", minLength: 1 },
      peers: {
        type: "array",
        items: peerInfoSchema,
      },
    },
  },

  peer_joined: {
    type: "object",
    required: ["lobby_id", "peer"],
    additionalProperties: false,
    properties: {
      lobby_id: { type: "string", minLength: 1 },
      peer: peerInfoSchema,
    },
  },

  peer_left: {
    type: "object",
    required: ["lobby_id", "session_id"],
    additionalProperties: false,
    properties: {
      lobby_id:   { type: "string", minLength: 1 },
      session_id: { type: "string", minLength: 1 },
    },
  },

  leave: {
    type: "object",
    additionalProperties: false,
    properties: {},
  },

  offer: {
    type: "object",
    required: ["target_session_id", "data"],
    additionalProperties: false,
    properties: {
      target_session_id: { type: "string", minLength: 1 },
      data: {},  // opaque SDP JSON — any shape is valid
    },
  },

  answer: {
    type: "object",
    required: ["target_session_id", "data"],
    additionalProperties: false,
    properties: {
      target_session_id: { type: "string", minLength: 1 },
      data: {},
    },
  },

  ice: {
    type: "object",
    required: ["target_session_id", "data"],
    additionalProperties: false,
    properties: {
      target_session_id: { type: "string", minLength: 1 },
      data: {},
    },
  },

  relay: {
    type: "object",
    required: ["from_session_id", "kind", "data"],
    additionalProperties: false,
    properties: {
      from_session_id: { type: "string", minLength: 1 },
      kind: { type: "string", enum: ["offer", "answer", "ice"] },
      data: {},
    },
  },

  error: errorPayloadSchema,

  ping: {
    type: "object",
    additionalProperties: false,
    properties: {},
  },

  pong: {
    type: "object",
    additionalProperties: false,
    properties: {},
  },
};

// ---------------------------------------------------------------------------
// Top-level Envelope schema (dynamic — validates by type)
// ---------------------------------------------------------------------------

const allTypes = Object.keys(payloadSchemas);

/**
 * JSON Schema (Draft-07) for an Envelope object.
 * The `payload` property is validated against the per-type schema selected by `type`.
 */
export const envelopeSchema = {
  $schema: "http://json-schema.org/draft-07/schema#",
  $id: "https://apeiron-cipher.internal/signaling/envelope",
  title: "SignalingEnvelope",
  description: `Apeiron Cipher signaling protocol envelope (schema v${SCHEMA_VERSION})`,
  type: "object",
  required: ["type"],
  additionalProperties: false,
  properties: {
    type: { type: "string", enum: allTypes },
    payload: {},  // refined per type in if/then blocks below
  },
  // Per-type payload validation via JSON Schema if/then
  allOf: allTypes.map((t) => ({
    if: { properties: { type: { const: t } }, required: ["type"] },
    then: { properties: { payload: payloadSchemas[t] } },
  })),
};

// ---------------------------------------------------------------------------
// AJV-based runtime validator (optional convenience export)
// ---------------------------------------------------------------------------

let _ajv: unknown | null = null;
let _validate: ((data: unknown) => boolean) | null = null;

function getValidator(): ((data: unknown) => boolean) {
  if (_validate) return _validate;
  try {
    // AJV is an optional peer dep — do not throw if absent
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const Ajv = require("ajv");
    const ajv = new Ajv({ strict: false });
    _ajv = ajv;
    _validate = ajv.compile(envelopeSchema);
    return _validate!;
  } catch {
    // AJV not installed — return a permissive stub
    return (_data: unknown) => true;
  }
}

/**
 * Validate a parsed JSON object against the signaling envelope schema.
 * Returns true if valid. Requires `ajv` package to be installed for real
 * validation; returns true unconditionally if ajv is absent.
 */
export function validateEnvelope(data: unknown): boolean {
  return getValidator()(data);
}

/**
 * Same as validateEnvelope but throws with a descriptive error on failure.
 */
export function assertEnvelope(data: unknown): void {
  const validate = getValidator();
  if (!validate(data)) {
    const err = (validate as unknown as { errors?: unknown[] }).errors;
    throw new Error(
      `Invalid signaling envelope: ${JSON.stringify(err ?? "unknown AJV error")}`,
    );
  }
}

export { payloadSchemas };
