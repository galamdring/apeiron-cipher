//! Matchmaking and signaling client for Apeiron Cipher multiplayer (Epic 22, Story 22.5).
//!
//! Provides [`MatchmakingPlugin`], which spawns a dedicated background Tokio thread that:
//!
//! - Opens a persistent WebSocket connection to the signaling server.
//! - Registers the client and recovers from disconnects with exponential back-off.
//! - Exposes Bevy commands ([`MatchmakingCommand`]) for lobby create / list / join.
//! - Fires Bevy events ([`MatchmakingEvent`]) when the server sends lobby or peer updates.
//! - Relays SDP offers, SDP answers, and ICE candidates between peers so that a
//!   [`webrtc::peer_connection::RTCPeerConnection`] can be established entirely from game code.
//!
//! # Architecture
//!
//! The Bevy main thread and the async Tokio thread communicate exclusively through a pair of
//! [`crossbeam_channel`] channels — one in each direction.  This keeps async logic off the ECS
//! loop and prevents any blocking calls from stalling rendering.
//!
//! ```text
//!  Bevy main thread                       Tokio background thread
//!  ────────────────                       ───────────────────────
//!  MatchmakingManager::send_command ──► cmd_tx ──► ws_loop
//!  handle_incoming_messages         ◄── evt_rx ◄── ws_loop + rtc handlers
//! ```
//!
//! # Determinism (Core Principle 4)
//!
//! This module performs no seeded random generation.  Peer negotiation ordering is determined
//! by which client sends the SDP offer first; the convention is that the player who joined
//! (not the host) sends the offer.  There is no game-state mutation at this layer that would
//! need to satisfy the same-seed determinism requirement.
//!
//! # Usage
//!
//! ```no_run
//! use apeiron_cipher::matchmaking::{MatchmakingCommand, MatchmakingConfig};
//! use bevy::prelude::*;
//!
//! fn send_list_request(manager: Res<apeiron_cipher::matchmaking::MatchmakingManager>) {
//!     manager.send_command(MatchmakingCommand::ListLobbies);
//! }
//! ```

use std::{sync::Arc, time::Duration};

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender, bounded};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;

// Re-export so callers can name the peer-connection type without depending on webrtc directly.
pub use webrtc::peer_connection::RTCPeerConnection;

// ── Wire types (mirror infra/orchestrator/internal/signaling/messages.go) ────

/// Top-level envelope for every signaling wire message.
///
/// The `type` field is the discriminant; `payload` carries type-specific JSON.
/// We keep payload as [`Option<Value>`] so the Bevy layer can forward raw SDP/ICE
/// without deserialising it (the server passes opaque blobs through unchanged).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Value>,
}

impl Envelope {
    fn new(msg_type: impl Into<String>, payload: impl Serialize) -> Self {
        Self {
            msg_type: msg_type.into(),
            payload: Some(serde_json::to_value(payload).expect("Envelope::new serialize")),
        }
    }

    fn no_payload(msg_type: impl Into<String>) -> Self {
        Self {
            msg_type: msg_type.into(),
            payload: None,
        }
    }
}

/// Lobby summary returned by [`MatchmakingEvent::LobbyList`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LobbyInfo {
    /// Unique lobby identifier assigned by the server.
    pub lobby_id: String,
    /// Display name of the lobby creator.
    pub display_name: String,
    /// Current number of peers including the creator.
    pub peer_count: u32,
    /// Maximum peers allowed; `0` means unlimited.
    #[serde(default)]
    pub max_peers: u32,
}

/// Minimal peer descriptor included in join / peer_joined messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Ephemeral session ID assigned by the signaling server.
    pub session_id: String,
    /// Human-readable display name for this peer.
    pub display_name: String,
}

/// SDP offer or answer payload relayed through the signaling server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdpPayload {
    /// Destination peer's session ID.
    pub target_session_id: String,
    /// Raw SDP string (offer or answer).
    pub sdp: String,
    /// `"offer"` or `"answer"`.
    pub kind: String,
}

/// ICE candidate payload relayed through the signaling server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IcePayload {
    /// Destination peer's session ID.
    pub target_session_id: String,
    /// ICE candidate init JSON (passed through unchanged).
    pub candidate: Value,
}

/// Relay delivery — what a peer receives when SDP or ICE is forwarded to it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayDelivery {
    /// Session ID of the sender.
    pub from_session_id: String,
    /// `"offer"`, `"answer"`, or `"ice"`.
    pub kind: String,
    /// Opaque SDP string or ICE candidate JSON.
    pub data: Value,
}

// ── Commands — Bevy → async thread ──────────────────────────────────────────

/// Commands that game systems can send to the matchmaking manager.
///
/// Send via [`MatchmakingManager::send_command`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum MatchmakingCommand {
    /// Register (or re-register) with the signaling server.
    Register {
        /// Optional human-readable display name shown to other players.
        display_name: Option<String>,
    },
    /// Request the current lobby list from the server.
    ListLobbies,
    /// Create a new lobby.
    CreateLobby {
        /// Maximum peers; `0` means unlimited.
        max_peers: u32,
    },
    /// Join an existing lobby by ID.
    JoinLobby {
        /// ID of the lobby to join.
        lobby_id: String,
    },
    /// Leave the current lobby (stays connected to the signaling server).
    LeaveLobby,
    /// Send an SDP offer to a peer.
    SendOffer(SdpPayload),
    /// Send an SDP answer to a peer.
    SendAnswer(SdpPayload),
    /// Send an ICE candidate to a peer.
    SendIce(IcePayload),
    /// Gracefully disconnect and shut down the background thread.
    Shutdown,
}

// ── Events — async thread → Bevy ────────────────────────────────────────────

/// Events fired into the Bevy event system by the matchmaking background thread.
///
/// Read with `MessageReader<MatchmakingEvent>`.
#[derive(Debug, Clone, Message)]
#[non_exhaustive]
pub enum MatchmakingEvent {
    /// Successfully registered; contains the assigned session ID.
    Registered {
        /// Ephemeral session ID for this connection.
        session_id: String,
        /// Display name echoed or assigned by the server.
        display_name: String,
    },
    /// Response to [`MatchmakingCommand::ListLobbies`].
    LobbyList(Vec<LobbyInfo>),
    /// A new lobby was created; contains the assigned lobby ID.
    LobbyCreated {
        /// The assigned lobby ID.
        lobby_id: String,
    },
    /// This client successfully joined a lobby.
    LobbyJoined {
        /// The joined lobby's ID.
        lobby_id: String,
        /// Existing peers in the lobby (excluding self).
        peers: Vec<PeerInfo>,
    },
    /// A new peer joined the client's lobby.
    PeerJoined {
        /// Lobby the peer joined.
        lobby_id: String,
        /// The new peer.
        peer: PeerInfo,
    },
    /// A peer left or disconnected from the lobby.
    PeerLeft {
        /// Lobby the peer left.
        lobby_id: String,
        /// Session ID of the departing peer.
        session_id: String,
    },
    /// A relayed SDP/ICE message arrived from a peer.
    RelayReceived(RelayDelivery),
    /// The server returned an error response.
    ServerError {
        /// Machine-readable error code.
        code: String,
        /// Human-readable error message.
        message: String,
    },
    /// The WebSocket connection was lost; the manager is attempting to reconnect.
    Disconnected,
    /// The WebSocket connection was re-established after a disconnect.
    Reconnected,
}

// ── Configuration ────────────────────────────────────────────────────────────

/// Configuration resource injected before the plugin starts.
///
/// Insert this as a resource before adding [`MatchmakingPlugin`] to override defaults.
#[derive(Debug, Clone, Resource)]
pub struct MatchmakingConfig {
    /// WebSocket URL of the signaling server, e.g. `"ws://localhost:8080/ws"`.
    pub server_url: String,
    /// Optional display name sent during registration.
    pub display_name: Option<String>,
    /// Maximum reconnect attempts before giving up (0 = retry forever).
    pub max_reconnect_attempts: u32,
    /// Base delay for exponential back-off between reconnect attempts.
    pub reconnect_base_delay: Duration,
    /// Interval between WebSocket keepalive pings.
    pub ping_interval: Duration,
}

impl Default for MatchmakingConfig {
    fn default() -> Self {
        Self {
            server_url: "ws://localhost:8080/ws".to_owned(),
            display_name: None,
            max_reconnect_attempts: 0,
            reconnect_base_delay: Duration::from_secs(1),
            ping_interval: Duration::from_secs(15),
        }
    }
}

// ── MatchmakingManager resource ──────────────────────────────────────────────

/// Bevy resource that owns the channel handles used to communicate with the
/// matchmaking background thread.
///
/// Inserted automatically by [`MatchmakingPlugin`].
#[derive(Resource)]
pub struct MatchmakingManager {
    cmd_tx: Sender<MatchmakingCommand>,
    evt_rx: Receiver<MatchmakingEvent>,
    /// Ephemeral session ID assigned after successful registration; `None` until registered.
    pub session_id: Option<String>,
}

impl MatchmakingManager {
    /// Send a command to the background matchmaking thread.
    ///
    /// Returns `false` if the background thread has exited.
    pub fn send_command(&self, cmd: MatchmakingCommand) -> bool {
        self.cmd_tx.send(cmd).is_ok()
    }

    /// Drain all pending events from the background thread without blocking.
    ///
    /// Prefer the [`handle_incoming_messages`] system which pumps this automatically.
    pub fn drain_events(&self) -> impl Iterator<Item = MatchmakingEvent> + '_ {
        self.evt_rx.try_iter()
    }
}

// ── Peer-connection registry ─────────────────────────────────────────────────

/// Bevy resource holding open [`RTCPeerConnection`] instances keyed by peer session ID.
///
/// Populated by [`MatchmakingPlugin`]'s systems when relay messages arrive.
/// Game code reads this to send data-channel messages once negotiation completes.
#[derive(Default, Resource)]
pub struct PeerConnections {
    connections: std::collections::HashMap<String, Arc<RTCPeerConnection>>,
}

impl PeerConnections {
    /// Retrieve a peer connection by session ID.
    pub fn get(&self, session_id: &str) -> Option<&Arc<RTCPeerConnection>> {
        self.connections.get(session_id)
    }

    /// Insert a peer connection; replaces any existing connection for that peer.
    pub fn insert(&mut self, session_id: String, pc: Arc<RTCPeerConnection>) {
        self.connections.insert(session_id, pc);
    }

    /// Remove a peer connection, closing it if no other arcs exist.
    pub fn remove(&mut self, session_id: &str) -> Option<Arc<RTCPeerConnection>> {
        self.connections.remove(session_id)
    }
}

// ── Plugin ───────────────────────────────────────────────────────────────────

/// Bevy plugin — wires the matchmaking and WebRTC signaling subsystem into the app.
///
/// Reads [`MatchmakingConfig`] if present (falls back to defaults), then:
///
/// 1. Spawns a background OS thread with its own [`tokio::runtime::Runtime`].
/// 2. Inserts [`MatchmakingManager`] and [`PeerConnections`] as resources.
/// 3. Registers [`MatchmakingEvent`] as a Bevy event.
/// 4. Adds the [`handle_incoming_messages`] system to `Update`.
pub struct MatchmakingPlugin;

impl Plugin for MatchmakingPlugin {
    fn build(&self, app: &mut App) {
        let config = app
            .world()
            .get_resource::<MatchmakingConfig>()
            .cloned()
            .unwrap_or_default();

        let (cmd_tx, cmd_rx) = bounded::<MatchmakingCommand>(64);
        let (evt_tx, evt_rx) = bounded::<MatchmakingEvent>(256);

        // Spawn the background thread that owns the Tokio runtime and WS connection.
        let config_clone = config.clone();
        std::thread::Builder::new()
            .name("matchmaking-bg".to_owned())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("matchmaking: failed to create Tokio runtime");
                rt.block_on(run_matchmaking_loop(config_clone, cmd_rx, evt_tx));
            })
            .expect("matchmaking: failed to spawn background thread");

        app.insert_resource(MatchmakingManager {
            cmd_tx,
            evt_rx,
            session_id: None,
        })
        .insert_resource(PeerConnections::default())
        .add_message::<MatchmakingEvent>()
        .add_systems(Update, handle_incoming_messages);
    }
}

// ── Bevy system — drain channel → Bevy events ────────────────────────────────

/// Drains the matchmaking event channel and forwards events into Bevy's message bus.
///
/// Also updates [`MatchmakingManager::session_id`] when a `Registered` event arrives.
pub fn handle_incoming_messages(
    mut manager: ResMut<MatchmakingManager>,
    mut writer: MessageWriter<MatchmakingEvent>,
) {
    let events: Vec<MatchmakingEvent> = manager.evt_rx.try_iter().collect();
    for evt in events {
        if let MatchmakingEvent::Registered { ref session_id, .. } = evt {
            manager.session_id = Some(session_id.clone());
        }
        writer.write(evt);
    }
}

// ── Async background loop ─────────────────────────────────────────────────────

/// Root async task for the matchmaking background thread.
///
/// Manages connection lifecycle with exponential back-off reconnect.
async fn run_matchmaking_loop(
    config: MatchmakingConfig,
    cmd_rx: Receiver<MatchmakingCommand>,
    evt_tx: Sender<MatchmakingEvent>,
) {
    let cmd_rx = Arc::new(AsyncMutex::new(cmd_rx));
    let mut attempt: u32 = 0;

    loop {
        match connect_and_run(&config, cmd_rx.clone(), evt_tx.clone()).await {
            LoopResult::Shutdown => {
                info!("matchmaking: shutdown requested — exiting background thread");
                break;
            }
            LoopResult::Disconnected => {
                if config.max_reconnect_attempts > 0 && attempt >= config.max_reconnect_attempts {
                    error!(
                        attempt,
                        "matchmaking: max reconnect attempts reached — giving up"
                    );
                    break;
                }
                let delay = config.reconnect_base_delay * 2u32.saturating_pow(attempt.min(6));
                warn!(
                    attempt,
                    ?delay,
                    "matchmaking: disconnected — reconnecting after delay"
                );
                let _ = evt_tx.send(MatchmakingEvent::Disconnected);
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

#[derive(Debug)]
enum LoopResult {
    Shutdown,
    Disconnected,
}

/// Establishes one WebSocket connection and runs until it closes or a shutdown arrives.
async fn connect_and_run(
    config: &MatchmakingConfig,
    cmd_rx: Arc<AsyncMutex<Receiver<MatchmakingCommand>>>,
    evt_tx: Sender<MatchmakingEvent>,
) -> LoopResult {
    use tokio_tungstenite::{connect_async, tungstenite::Message};

    let ws_stream = match connect_async(&config.server_url).await {
        Ok((ws, _)) => ws,
        Err(err) => {
            error!(%err, url = %config.server_url, "matchmaking: WebSocket connect failed");
            return LoopResult::Disconnected;
        }
    };

    info!(url = %config.server_url, "matchmaking: WebSocket connected");

    // Send initial registration.
    let (mut write_half, mut read_half) = ws_stream.split();
    let reg_env = Envelope::new(
        "register",
        serde_json::json!({
            "display_name": config.display_name.as_deref().unwrap_or("")
        }),
    );
    if let Err(err) = write_half
        .send(Message::Text(
            serde_json::to_string(&reg_env)
                .expect("register envelope serialize")
                .into(),
        ))
        .await
    {
        error!(%err, "matchmaking: failed to send register message");
        return LoopResult::Disconnected;
    }

    let mut ping_interval = tokio::time::interval(config.ping_interval);
    ping_interval.tick().await; // consume the immediate tick

    loop {
        tokio::select! {
            // Inbound message from the signaling server.
            maybe_msg = read_half.next() => {
                match maybe_msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<Envelope>(&text) {
                            Ok(env) => {
                                if let Some(evt) = dispatch_server_message(env) {
                                    let _ = evt_tx.send(evt);
                                }
                            }
                            Err(err) => {
                                warn!(%err, raw = %text, "matchmaking: unrecognised message");
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = write_half.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        warn!("matchmaking: server closed WebSocket");
                        return LoopResult::Disconnected;
                    }
                    Some(Ok(_)) => {} // binary / pong — ignore
                    Some(Err(err)) => {
                        error!(%err, "matchmaking: WebSocket read error");
                        return LoopResult::Disconnected;
                    }
                }
            }

            // Outbound command from a Bevy system.
            cmd = async {
                let rx = cmd_rx.lock().await;
                // Non-blocking try_recv so we don't hold the lock across an await.
                rx.try_recv().ok()
            } => {
                if let Some(cmd) = cmd {
                    match cmd {
                        MatchmakingCommand::Shutdown => return LoopResult::Shutdown,
                        cmd => {
                            if let Some(env) = command_to_envelope(cmd) {
                                let text = serde_json::to_string(&env)
                                    .expect("command envelope serialize");
                                if let Err(err) = write_half.send(Message::Text(text.into())).await {
                                    error!(%err, "matchmaking: send failed");
                                    return LoopResult::Disconnected;
                                }
                            }
                        }
                    }
                } else {
                    // No command ready — yield to other branches.
                    tokio::task::yield_now().await;
                }
            }

            // Keepalive ping.
            _ = ping_interval.tick() => {
                let ping_env = Envelope::no_payload("ping");
                let text = serde_json::to_string(&ping_env).expect("ping serialize");
                if let Err(err) = write_half.send(Message::Text(text.into())).await {
                    error!(%err, "matchmaking: ping send failed");
                    return LoopResult::Disconnected;
                }
            }
        }
    }
}

/// Map an inbound [`Envelope`] to a [`MatchmakingEvent`], if applicable.
fn dispatch_server_message(env: Envelope) -> Option<MatchmakingEvent> {
    let payload = env.payload.unwrap_or(Value::Null);
    match env.msg_type.as_str() {
        "registered" => Some(MatchmakingEvent::Registered {
            session_id: payload["session_id"].as_str().unwrap_or("").to_owned(),
            display_name: payload["display_name"].as_str().unwrap_or("").to_owned(),
        }),
        "lobby_list" => {
            let lobbies: Vec<LobbyInfo> =
                serde_json::from_value(payload["lobbies"].clone()).unwrap_or_default();
            Some(MatchmakingEvent::LobbyList(lobbies))
        }
        "lobby_created" => Some(MatchmakingEvent::LobbyCreated {
            lobby_id: payload["lobby_id"].as_str().unwrap_or("").to_owned(),
        }),
        "lobby_joined" => {
            let peers: Vec<PeerInfo> =
                serde_json::from_value(payload["peers"].clone()).unwrap_or_default();
            Some(MatchmakingEvent::LobbyJoined {
                lobby_id: payload["lobby_id"].as_str().unwrap_or("").to_owned(),
                peers,
            })
        }
        "peer_joined" => {
            let peer: PeerInfo =
                serde_json::from_value(payload["peer"].clone()).unwrap_or(PeerInfo {
                    session_id: String::new(),
                    display_name: String::new(),
                });
            Some(MatchmakingEvent::PeerJoined {
                lobby_id: payload["lobby_id"].as_str().unwrap_or("").to_owned(),
                peer,
            })
        }
        "peer_left" => Some(MatchmakingEvent::PeerLeft {
            lobby_id: payload["lobby_id"].as_str().unwrap_or("").to_owned(),
            session_id: payload["session_id"].as_str().unwrap_or("").to_owned(),
        }),
        "relay" => {
            let delivery: RelayDelivery = serde_json::from_value(payload).ok()?;
            Some(MatchmakingEvent::RelayReceived(delivery))
        }
        "error" => Some(MatchmakingEvent::ServerError {
            code: payload["code"].as_str().unwrap_or("unknown").to_owned(),
            message: payload["message"].as_str().unwrap_or("").to_owned(),
        }),
        "pong" => None, // keepalive response — no Bevy event needed
        other => {
            warn!(msg_type = %other, "matchmaking: unknown server message type");
            None
        }
    }
}

/// Convert a [`MatchmakingCommand`] into a wire [`Envelope`].
fn command_to_envelope(cmd: MatchmakingCommand) -> Option<Envelope> {
    match cmd {
        MatchmakingCommand::Register { display_name } => Some(Envelope::new(
            "register",
            serde_json::json!({ "display_name": display_name.unwrap_or_default() }),
        )),
        MatchmakingCommand::ListLobbies => Some(Envelope::no_payload("list_lobbies")),
        MatchmakingCommand::CreateLobby { max_peers } => Some(Envelope::new(
            "create_lobby",
            serde_json::json!({ "max_peers": max_peers }),
        )),
        MatchmakingCommand::JoinLobby { lobby_id } => Some(Envelope::new(
            "join_lobby",
            serde_json::json!({ "lobby_id": lobby_id }),
        )),
        MatchmakingCommand::LeaveLobby => Some(Envelope::no_payload("leave")),
        MatchmakingCommand::SendOffer(sdp) => Some(Envelope::new(
            "offer",
            serde_json::json!({
                "target_session_id": sdp.target_session_id,
                "data": { "type": "offer", "sdp": sdp.sdp },
            }),
        )),
        MatchmakingCommand::SendAnswer(sdp) => Some(Envelope::new(
            "answer",
            serde_json::json!({
                "target_session_id": sdp.target_session_id,
                "data": { "type": "answer", "sdp": sdp.sdp },
            }),
        )),
        MatchmakingCommand::SendIce(ice) => Some(Envelope::new(
            "ice",
            serde_json::json!({
                "target_session_id": ice.target_session_id,
                "data": ice.candidate,
            }),
        )),
        MatchmakingCommand::Shutdown => None, // handled before reaching here
    }
}

// ── WebRTC helpers ────────────────────────────────────────────────────────────

/// Build a default [`RTCPeerConnection`] pre-configured for game use.
///
/// Uses a single STUN server (`stun:stun.l.google.com:19302`) and enables a
/// reliable ordered data channel suitable for game state exchange.
///
/// # Errors
///
/// Returns an error if the WebRTC runtime cannot initialise (rare — only on
/// platforms with no network stack).
pub async fn create_peer_connection() -> Result<Arc<RTCPeerConnection>, webrtc::Error> {
    use webrtc::{
        api::{
            APIBuilder, interceptor_registry::register_default_interceptors,
            media_engine::MediaEngine,
        },
        ice_transport::ice_server::RTCIceServer,
        interceptor::registry::Registry,
        peer_connection::configuration::RTCConfiguration,
    };

    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;

    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)?;

    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .build();

    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let pc = api.new_peer_connection(config).await?;
    Ok(Arc::new(pc))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Envelope serialisation ────────────────────────────────────────────────

    /// Envelope with a payload round-trips through JSON.
    #[test]
    fn envelope_with_payload_serializes() {
        let env = Envelope::new("register", serde_json::json!({ "display_name": "Alice" }));
        let json = serde_json::to_string(&env).unwrap();
        let back: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back.msg_type, "register");
        assert_eq!(
            back.payload.unwrap()["display_name"].as_str().unwrap(),
            "Alice"
        );
    }

    /// Envelope without a payload omits the `payload` field on the wire.
    #[test]
    fn envelope_no_payload_omits_field() {
        let env = Envelope::no_payload("ping");
        let json = serde_json::to_string(&env).unwrap();
        assert!(
            !json.contains("payload"),
            "payload should be absent: {json}"
        );
        let back: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back.msg_type, "ping");
        assert!(back.payload.is_none());
    }

    // ── command_to_envelope ───────────────────────────────────────────────────

    /// ListLobbies produces an envelope with type `"list_lobbies"` and no payload.
    #[test]
    fn list_lobbies_command_envelope() {
        let env = command_to_envelope(MatchmakingCommand::ListLobbies).unwrap();
        assert_eq!(env.msg_type, "list_lobbies");
        assert!(env.payload.is_none());
    }

    /// CreateLobby encodes `max_peers` in the payload.
    #[test]
    fn create_lobby_command_encodes_max_peers() {
        let env = command_to_envelope(MatchmakingCommand::CreateLobby { max_peers: 4 }).unwrap();
        assert_eq!(env.msg_type, "create_lobby");
        assert_eq!(env.payload.unwrap()["max_peers"], 4);
    }

    /// JoinLobby encodes the lobby ID.
    #[test]
    fn join_lobby_command_encodes_lobby_id() {
        let env = command_to_envelope(MatchmakingCommand::JoinLobby {
            lobby_id: "lobby-abc".to_owned(),
        })
        .unwrap();
        assert_eq!(env.msg_type, "join_lobby");
        assert_eq!(
            env.payload.unwrap()["lobby_id"].as_str().unwrap(),
            "lobby-abc"
        );
    }

    /// Shutdown produces `None` — it is never sent over the wire.
    #[test]
    fn shutdown_command_produces_no_envelope() {
        assert!(command_to_envelope(MatchmakingCommand::Shutdown).is_none());
    }

    /// SendOffer wraps SDP data in the expected relay shape.
    #[test]
    fn send_offer_command_shape() {
        let env = command_to_envelope(MatchmakingCommand::SendOffer(SdpPayload {
            target_session_id: "peer-1".to_owned(),
            sdp: "v=0\r\n...".to_owned(),
            kind: "offer".to_owned(),
        }))
        .unwrap();
        assert_eq!(env.msg_type, "offer");
        let p = env.payload.unwrap();
        assert_eq!(p["target_session_id"].as_str().unwrap(), "peer-1");
        assert_eq!(p["data"]["type"].as_str().unwrap(), "offer");
    }

    // ── dispatch_server_message ───────────────────────────────────────────────

    /// `registered` envelope fires a `Registered` event with session_id and display_name.
    #[test]
    fn dispatch_registered() {
        let env = Envelope::new(
            "registered",
            serde_json::json!({ "session_id": "s1", "display_name": "Alice" }),
        );
        let evt = dispatch_server_message(env).unwrap();
        match evt {
            MatchmakingEvent::Registered {
                session_id,
                display_name,
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(display_name, "Alice");
            }
            other => panic!("expected Registered, got {other:?}"),
        }
    }

    /// `lobby_list` with an empty lobbies array produces `LobbyList([])`.
    #[test]
    fn dispatch_lobby_list_empty() {
        let env = Envelope::new("lobby_list", serde_json::json!({ "lobbies": [] }));
        let evt = dispatch_server_message(env).unwrap();
        match evt {
            MatchmakingEvent::LobbyList(list) => assert!(list.is_empty()),
            other => panic!("expected LobbyList, got {other:?}"),
        }
    }

    /// `lobby_list` with entries deserialises all `LobbyInfo` fields.
    #[test]
    fn dispatch_lobby_list_with_entries() {
        let env = Envelope::new(
            "lobby_list",
            serde_json::json!({
                "lobbies": [
                    { "lobby_id": "lx", "display_name": "Host", "peer_count": 2, "max_peers": 4 }
                ]
            }),
        );
        let evt = dispatch_server_message(env).unwrap();
        match evt {
            MatchmakingEvent::LobbyList(list) => {
                assert_eq!(list.len(), 1);
                assert_eq!(list[0].lobby_id, "lx");
                assert_eq!(list[0].peer_count, 2);
                assert_eq!(list[0].max_peers, 4);
            }
            other => panic!("expected LobbyList, got {other:?}"),
        }
    }

    /// `peer_joined` deserialises lobby_id and peer info.
    #[test]
    fn dispatch_peer_joined() {
        let env = Envelope::new(
            "peer_joined",
            serde_json::json!({
                "lobby_id": "lobby-1",
                "peer": { "session_id": "s2", "display_name": "Bob" }
            }),
        );
        let evt = dispatch_server_message(env).unwrap();
        match evt {
            MatchmakingEvent::PeerJoined { lobby_id, peer } => {
                assert_eq!(lobby_id, "lobby-1");
                assert_eq!(peer.session_id, "s2");
                assert_eq!(peer.display_name, "Bob");
            }
            other => panic!("expected PeerJoined, got {other:?}"),
        }
    }

    /// `error` envelope maps to `ServerError` with code and message.
    #[test]
    fn dispatch_error() {
        let env = Envelope::new(
            "error",
            serde_json::json!({ "code": "not_registered", "message": "Please register first" }),
        );
        let evt = dispatch_server_message(env).unwrap();
        match evt {
            MatchmakingEvent::ServerError { code, message } => {
                assert_eq!(code, "not_registered");
                assert_eq!(message, "Please register first");
            }
            other => panic!("expected ServerError, got {other:?}"),
        }
    }

    /// `pong` produces no event (keepalive response).
    #[test]
    fn dispatch_pong_returns_none() {
        let env = Envelope::no_payload("pong");
        assert!(dispatch_server_message(env).is_none());
    }

    /// Unknown message type produces no event and does not panic.
    #[test]
    fn dispatch_unknown_type_returns_none() {
        let env = Envelope::no_payload("future_message_type_v99");
        assert!(dispatch_server_message(env).is_none());
    }

    // ── MatchmakingConfig defaults ────────────────────────────────────────────

    /// Default config uses localhost signaling URL with sane retry parameters.
    #[test]
    fn config_defaults_are_sane() {
        let cfg = MatchmakingConfig::default();
        assert!(cfg.server_url.starts_with("ws://"));
        assert!(cfg.reconnect_base_delay.as_secs() >= 1);
        assert!(cfg.ping_interval.as_secs() >= 10);
    }

    // ── Channel-based manager integration ────────────────────────────────────

    /// Sending a command through the channel succeeds when a receiver exists.
    #[test]
    fn manager_send_command_succeeds() {
        let (cmd_tx, cmd_rx) = bounded::<MatchmakingCommand>(4);
        let (_, evt_rx) = bounded::<MatchmakingEvent>(4);
        let manager = MatchmakingManager {
            cmd_tx,
            evt_rx,
            session_id: None,
        };
        assert!(manager.send_command(MatchmakingCommand::ListLobbies));
        // Verify the command arrived.
        let cmd = cmd_rx.try_recv().unwrap();
        assert!(matches!(cmd, MatchmakingCommand::ListLobbies));
    }

    /// Sending after the receiver is dropped returns `false`.
    #[test]
    fn manager_send_command_after_drop_returns_false() {
        let (cmd_tx, cmd_rx) = bounded::<MatchmakingCommand>(4);
        let (_, evt_rx) = bounded::<MatchmakingEvent>(4);
        let manager = MatchmakingManager {
            cmd_tx,
            evt_rx,
            session_id: None,
        };
        drop(cmd_rx);
        assert!(!manager.send_command(MatchmakingCommand::ListLobbies));
    }

    /// Events sent on the evt channel are drained by `drain_events`.
    #[test]
    fn manager_drain_events_returns_pending_events() {
        let (cmd_tx, _cmd_rx) = bounded::<MatchmakingCommand>(4);
        let (evt_tx, evt_rx) = bounded::<MatchmakingEvent>(4);
        let manager = MatchmakingManager {
            cmd_tx,
            evt_rx,
            session_id: None,
        };
        evt_tx.send(MatchmakingEvent::LobbyList(vec![])).unwrap();
        evt_tx
            .send(MatchmakingEvent::LobbyCreated {
                lobby_id: "x".to_owned(),
            })
            .unwrap();
        let events: Vec<_> = manager.drain_events().collect();
        assert_eq!(events.len(), 2);
    }
}
