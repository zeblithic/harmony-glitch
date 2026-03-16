pub mod avatar;
pub mod engine;
pub mod identity;
pub mod item;
pub mod network;
pub mod physics;
pub mod street;

use avatar::types::Direction;
use engine::state::GameState;
use network::state::{NetworkAction, NetworkState};
use network::transport::{UdpTransport, DEFAULT_PORT};
use network::types::PlayerNetState;
use physics::movement::InputState;
use street::parser::parse_street;
use street::types::StreetData;

use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Shared monotonic epoch — all time fed to NetworkState is relative to this.
struct MonotonicEpoch(Instant);

/// Shared game state protected by a mutex.
struct GameStateWrapper(Mutex<GameState>);

/// Shared input state — written by frontend, read by game loop.
struct InputStateWrapper(Mutex<InputState>);

/// Flag to control the game loop.
struct GameRunning(Mutex<bool>);

/// Handle to the game loop thread — joined before spawning a new one
/// to prevent concurrent loops from a rapid stop→start sequence.
struct GameLoopHandle(Mutex<Option<JoinHandle<()>>>);

/// Player identity and display name, loaded from disk on startup.
struct PlayerIdentityWrapper {
    identity: Mutex<harmony_identity::PrivateIdentity>,
    display_name: Mutex<String>,
    setup_complete: Mutex<bool>,
    data_dir: std::path::PathBuf,
}

/// Shared network state — driven by the game loop, queried by commands.
struct NetworkWrapper(Mutex<NetworkState>);

/// Shared UDP transport — owned by the game loop, used for sends.
struct TransportWrapper(Mutex<UdpTransport>);

#[tauri::command]
fn list_streets() -> Vec<String> {
    // For Phase A: return hardcoded demo street names.
    // Later: scan assets directory or query content network.
    vec!["demo_meadow".to_string(), "demo_heights".to_string()]
}

#[tauri::command]
fn load_street(name: String, app: AppHandle) -> Result<StreetData, String> {
    // Load XML from bundled assets
    let xml = load_street_xml(&name)?;
    let street_data = parse_street(&xml)?;

    // Update game state
    {
        let state_wrapper = app.state::<GameStateWrapper>();
        let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
        state.load_street(street_data.clone());
    }

    // Update network state for the new street.
    // Use monotonic time relative to app start — must match the game loop's time source.
    let actions = {
        let net = app.state::<NetworkWrapper>();
        let mut net_state = net.0.lock().map_err(|e| e.to_string())?;
        let epoch = app.state::<MonotonicEpoch>();
        let now_secs = epoch.0.elapsed().as_secs_f64();
        // Use the canonical TSID from parsed street data, not the raw input name.
        // This ensures peers using short names ("demo_meadow") and TSIDs ("LADEMO001")
        // resolve to the same street identity for peer discovery.
        net_state.change_street(&street_data.tsid, now_secs, &mut rand::rngs::OsRng)?
    };
    execute_network_actions(&app, actions);

    Ok(street_data)
}

#[tauri::command]
fn send_input(input: InputState, app: AppHandle) -> Result<(), String> {
    let input_wrapper = app.state::<InputStateWrapper>();
    let mut current = input_wrapper.0.lock().map_err(|e| e.to_string())?;
    *current = input;
    Ok(())
}

#[tauri::command]
fn start_game(app: AppHandle) -> Result<(), String> {
    let running = app.state::<GameRunning>();
    let mut is_running = running.0.lock().map_err(|e| e.to_string())?;
    if *is_running {
        return Ok(()); // Already running
    }
    *is_running = true;
    drop(is_running);

    // Take handle out from under the lock, then join outside it.
    // Joining while holding the lock would block stop_game from acquiring it.
    let handle_wrapper = app.state::<GameLoopHandle>();
    let old_handle = {
        let mut handle = handle_wrapper.0.lock().map_err(|e| e.to_string())?;
        handle.take()
    };
    if let Some(h) = old_handle {
        let _ = h.join();
    }

    let app_handle = app.clone();
    {
        let mut handle = handle_wrapper.0.lock().map_err(|e| e.to_string())?;
        *handle = Some(std::thread::spawn(move || {
            game_loop(app_handle.clone());
            // Signal stopped on any exit (normal or panic unwind) so the
            // frontend doesn't hang waiting for render_frame events.
            if let Ok(mut running) = app_handle.state::<GameRunning>().0.lock() {
                *running = false;
            }
        }));
    }

    Ok(())
}

#[tauri::command]
fn stop_game(app: AppHandle) -> Result<(), String> {
    // Reset input so the next session doesn't inherit stale key state.
    {
        let input_wrapper = app.state::<InputStateWrapper>();
        let mut input = input_wrapper.0.lock().map_err(|e| e.to_string())?;
        *input = InputState::default();
    }

    let running = app.state::<GameRunning>();
    let mut is_running = running.0.lock().map_err(|e| e.to_string())?;
    *is_running = false;
    drop(is_running);

    // Take handle out from under the lock, then join outside it.
    // Joining while holding the lock would block start_game from acquiring it.
    let handle_wrapper = app.state::<GameLoopHandle>();
    let old_handle = {
        let mut handle = handle_wrapper.0.lock().map_err(|e| e.to_string())?;
        handle.take()
    };
    if let Some(h) = old_handle {
        let _ = h.join();
    }
    Ok(())
}

#[tauri::command]
fn get_identity(app: AppHandle) -> Result<serde_json::Value, String> {
    let pi = app.state::<PlayerIdentityWrapper>();
    let identity = pi.identity.lock().map_err(|e| e.to_string())?;
    let name = pi.display_name.lock().map_err(|e| e.to_string())?;
    let setup_complete = pi.setup_complete.lock().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "displayName": *name,
        "addressHash": hex::encode(identity.public_identity().address_hash),
        "setupComplete": *setup_complete,
    }))
}

#[tauri::command]
fn set_display_name(name: String, app: AppHandle) -> Result<(), String> {
    // Enforce server-side length limit (frontend has maxlength="30" but IPC
    // callers can bypass it). Truncate on char boundary to avoid partial emoji.
    let name = {
        let mut s = String::new();
        for ch in name.chars() {
            if s.len() + ch.len_utf8() > 30 {
                break;
            }
            s.push(ch);
        }
        s
    };
    if name.is_empty() {
        return Err("display name must not be empty".to_string());
    }

    // Extract what we need under the identity locks, then drop them
    // before disk I/O and network updates to minimize lock contention.
    let pi = app.state::<PlayerIdentityWrapper>();
    let profile = {
        // Lock identity FIRST to match get_identity ordering (prevents ABBA deadlock).
        let identity = pi.identity.lock().map_err(|e| e.to_string())?;
        let mut display_name = pi.display_name.lock().map_err(|e| e.to_string())?;
        let mut setup_complete = pi.setup_complete.lock().map_err(|e| e.to_string())?;
        *display_name = name.clone();
        *setup_complete = true;
        // Wrap hex-encoded key in Zeroizing so it's wiped on drop.
        let identity_hex = zeroize::Zeroizing::new(hex::encode(identity.to_private_bytes()));
        identity::persistence::PlayerProfile {
            identity_hex: (*identity_hex).clone(),
            display_name: name.clone(),
            setup_complete: true,
        }
        // identity, display_name, setup_complete locks dropped here;
        // identity_hex is zeroized on drop.
    };

    // Disk I/O outside identity lock scope.
    let json = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;
    identity::persistence::write_profile(&pi.data_dir.join("profile.json"), &json)?;

    // Propagate to NetworkState and trigger immediate re-announce.
    let epoch = app.state::<MonotonicEpoch>();
    let now_secs = epoch.0.elapsed().as_secs_f64();
    let actions = {
        let net = app.state::<NetworkWrapper>();
        let mut net_state = net.0.lock().map_err(|e| e.to_string())?;
        net_state.set_display_name(name, now_secs, &mut rand::rngs::OsRng)?
    };
    execute_network_actions(&app, actions);

    Ok(())
}

#[tauri::command]
fn send_chat(message: String, app: AppHandle) -> Result<(), String> {
    let actions = {
        let net = app.state::<NetworkWrapper>();
        let mut net_state = net.0.lock().map_err(|e| e.to_string())?;
        net_state.send_chat(message)
    };
    execute_network_actions(&app, actions);
    Ok(())
}

#[tauri::command]
fn get_network_status(app: AppHandle) -> Result<serde_json::Value, String> {
    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "peerCount": net_state.peer_count(),
    }))
}

/// Execute network actions by broadcasting packets via the UDP transport.
fn execute_network_actions(app: &AppHandle, actions: Vec<NetworkAction>) {
    for action in actions {
        match action {
            NetworkAction::SendPacket {
                interface_name,
                data,
            } => {
                debug_assert_eq!(
                    interface_name, "udp0",
                    "received SendPacket for unexpected interface {interface_name}"
                );
                let transport = app.state::<TransportWrapper>();
                let guard = transport.0.lock();
                if let Ok(t) = guard {
                    let _ = t.broadcast(&data, DEFAULT_PORT);
                }
            }
            NetworkAction::ChatReceived(chat) => {
                let _ = app.emit(
                    "chat_message",
                    serde_json::json!({
                        "text": chat.text,
                        "senderHash": hex::encode(chat.sender),
                        "senderName": chat.sender_name,
                    }),
                );
            }
            // Informational — registry updates happen inside NetworkState::tick
            // before these actions are emitted. No action needed here.
            NetworkAction::PresenceChange(_) | NetworkAction::RemotePlayerUpdate { .. } => {}
        }
    }
}

fn game_loop(app: AppHandle) {
    let tick_duration = Duration::from_secs_f64(1.0 / 60.0);
    let dt = 1.0 / 60.0;
    let game_start = app.state::<MonotonicEpoch>().0;
    let mut rng = rand::rngs::ThreadRng::default();

    loop {
        let tick_start = Instant::now();

        // 1. Check if still running
        {
            let running = app.state::<GameRunning>();
            let is_running = running.0.lock().unwrap_or_else(|e| e.into_inner());
            if !*is_running {
                break;
            }
        }

        // 2. Drain inbound UDP packets (non-blocking)
        let inbound_packets: Vec<(String, Vec<u8>)> = {
            let transport = app.state::<TransportWrapper>();
            let mut t = transport.0.lock().unwrap_or_else(|e| e.into_inner());
            t.recv_all()
                .into_iter()
                .map(|(data, _addr)| ("udp0".to_string(), data))
                .collect()
        };

        // 3. Tick NetworkState with packets + monotonic seconds (f64 for sub-second precision)
        let now_secs = tick_start.duration_since(game_start).as_secs_f64();
        let net_actions = {
            let net = app.state::<NetworkWrapper>();
            let mut net_state = net.0.lock().unwrap_or_else(|e| e.into_inner());
            net_state.tick(&inbound_packets, now_secs, &mut rng)
        };

        // 4. Execute NetworkActions (broadcast packets)
        execute_network_actions(&app, net_actions);

        // 5. Read current input
        let input = {
            let input_wrapper = app.state::<InputStateWrapper>();
            let guard = input_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());
            *guard
        };

        // 6. Tick game state — lock is scoped so it is released before emitting
        let frame = {
            let state_wrapper = app.state::<GameStateWrapper>();
            let mut state = state_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());
            state.tick(dt, &input)
        };

        if let Some(mut frame) = frame {
            // 7. Augment RenderFrame with remote players from NetworkState
            {
                let net = app.state::<NetworkWrapper>();
                let net_state = net.0.lock().unwrap_or_else(|e| e.into_inner());
                frame.remote_players = net_state.remote_frames();
            }

            // 8. Publish local player state via NetworkState
            let publish_actions = {
                let net_state = PlayerNetState {
                    x: frame.player.x as f32,
                    y: frame.player.y as f32,
                    vx: frame.player.vx as f32,
                    vy: frame.player.vy as f32,
                    facing: if frame.player.facing == Direction::Left {
                        0
                    } else {
                        1
                    },
                    on_ground: frame.player.on_ground,
                };
                let net = app.state::<NetworkWrapper>();
                let mut ns = net.0.lock().unwrap_or_else(|e| e.into_inner());
                ns.publish_player_state(&net_state)
            };
            execute_network_actions(&app, publish_actions);

            // 9. Emit RenderFrame to frontend
            let _ = app.emit("render_frame", &frame);
        }

        // 10. Sleep for remainder of tick
        let elapsed = tick_start.elapsed();
        if elapsed < tick_duration {
            std::thread::sleep(tick_duration - elapsed);
        }
    }
}

fn load_street_xml(name: &str) -> Result<String, String> {
    // Phase A: embed street XML at compile time so the binary is self-contained
    // and doesn't depend on CARGO_MANIFEST_DIR paths at runtime.
    // Accepts both short names ("demo_meadow") and TSIDs ("LADEMO001") since
    // signpost connections reference streets by TSID.
    match name {
        "demo_meadow" | "LADEMO001" => {
            Ok(include_str!("../../assets/streets/demo_meadow.xml").to_string())
        }
        "demo_heights" | "LADEMO002" => {
            Ok(include_str!("../../assets/streets/demo_heights.xml").to_string())
        }
        _ => Err(format!("Unknown street: {}", name)),
    }
}

pub fn run() {
    tauri::Builder::default()
        .manage(MonotonicEpoch(Instant::now()))
        .manage(GameStateWrapper(Mutex::new(GameState::new(1280.0, 720.0))))
        .manage(InputStateWrapper(Mutex::new(InputState::default())))
        .manage(GameRunning(Mutex::new(false)))
        .manage(GameLoopHandle(Mutex::new(None)))
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let (player_identity, display_name, setup_complete) =
                identity::persistence::load_or_create_profile(&data_dir)
                    .map_err(std::io::Error::other)?;

            // Save identity bytes BEFORE moving identity into PlayerIdentityWrapper,
            // since PrivateIdentity is not Clone.
            let identity_bytes = zeroize::Zeroizing::new(player_identity.to_private_bytes());

            app.manage(PlayerIdentityWrapper {
                identity: Mutex::new(player_identity),
                display_name: Mutex::new(display_name.clone()),
                setup_complete: Mutex::new(setup_complete),
                data_dir: data_dir.clone(),
            });

            // Reconstruct a second identity from saved bytes for NetworkState.
            let net_identity =
                harmony_identity::PrivateIdentity::from_private_bytes(identity_bytes.as_ref())
                    .map_err(|e| std::io::Error::other(format!("{e:?}")))?;
            app.manage(NetworkWrapper(Mutex::new(NetworkState::new(
                net_identity,
                display_name,
            ))));

            // Bind UDP transport for LAN discovery.
            let transport = UdpTransport::bind(DEFAULT_PORT)
                .map_err(|e| std::io::Error::other(format!("UDP bind failed: {e}")))?;
            app.manage(TransportWrapper(Mutex::new(transport)));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_streets,
            load_street,
            send_input,
            start_game,
            stop_game,
            get_identity,
            set_display_name,
            send_chat,
            get_network_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running harmony-glitch");
}
