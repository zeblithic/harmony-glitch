pub mod avatar;
pub mod engine;
pub mod identity;
pub mod item;
pub mod network;
pub mod persistence;
pub mod physics;
pub mod street;
pub mod trade;

use avatar::types::{AnimationState, AvatarAppearance, Direction};
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
use tauri::http;

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

/// Path to the sound-kits directory, created on startup.
struct SoundKitsDir(std::path::PathBuf);

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SoundKitMeta {
    id: String,
    name: String,
}

/// Shared trade state — manages active P2P trades.
struct TradeWrapper(Mutex<trade::state::TradeManager>);

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

/// Validate a sound kit ID: alphanumeric, hyphens, and underscores only.
/// Rejects path traversal attempts.
fn validate_kit_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("Kit ID must not be empty".to_string());
    }
    if id.contains('.') || id.contains('/') || id.contains('\\') {
        return Err(format!("Invalid kit ID: {id}"));
    }
    if !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(format!("Invalid kit ID: {id}"));
    }
    Ok(())
}

#[tauri::command]
fn list_sound_kits(app: AppHandle) -> Result<Vec<SoundKitMeta>, String> {
    let kits_dir = app.state::<SoundKitsDir>();
    let mut kits = vec![SoundKitMeta {
        id: "default".to_string(),
        name: "Default".to_string(),
    }];

    let entries = match std::fs::read_dir(&kits_dir.0) {
        Ok(e) => e,
        Err(_) => return Ok(kits),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let kit_json = path.join("kit.json");
        if !kit_json.exists() {
            continue;
        }
        let content = match std::fs::read_to_string(&kit_json) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[sound-kits] Failed to read {}: {e}", kit_json.display());
                continue;
            }
        };
        let parsed: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[sound-kits] Invalid JSON in {}: {e}", kit_json.display());
                continue;
            }
        };
        let name = parsed
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unnamed")
            .to_string();
        let id = entry.file_name().to_string_lossy().to_string();
        // Skip directories whose names aren't valid kit IDs (e.g. dots, spaces)
        // and skip "default" to avoid shadowing the built-in kit.
        if id == "default" || validate_kit_id(&id).is_err() {
            continue;
        }
        kits.push(SoundKitMeta { id, name });
    }

    // Sort custom kits alphabetically by name (default stays first)
    kits[1..].sort_by(|a, b| a.name.cmp(&b.name));

    Ok(kits)
}

#[tauri::command]
fn read_sound_kit(kit_id: String, app: AppHandle) -> Result<serde_json::Value, String> {
    if kit_id == "default" {
        let kit: serde_json::Value =
            serde_json::from_str(include_str!("../../assets/audio/default-kit.json"))
                .map_err(|e| format!("Failed to parse bundled kit: {e}"))?;
        return Ok(kit);
    }

    validate_kit_id(&kit_id)?;

    let kits_dir = app.state::<SoundKitsDir>();
    let kit_path = kits_dir.0.join(&kit_id).join("kit.json");

    let content =
        std::fs::read_to_string(&kit_path).map_err(|e| format!("Kit '{kit_id}' not found: {e}"))?;
    let kit: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Invalid kit manifest: {e}"))?;

    Ok(kit)
}

#[tauri::command]
fn get_saved_state(app: AppHandle) -> Result<Option<serde_json::Value>, String> {
    let pi = app.state::<PlayerIdentityWrapper>();
    let save_path = pi.data_dir.join("savegame.json");
    let save = crate::engine::state::read_save_state(&save_path)?;
    match save {
        Some(s) => {
            if load_street_xml(&s.street_id).is_err() {
                return Ok(None);
            }
            Ok(Some(serde_json::to_value(&s).map_err(|e| e.to_string())?))
        }
        None => Ok(None),
    }
}

#[tauri::command]
fn get_avatar(app: AppHandle) -> Result<AvatarAppearance, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    Ok(state.avatar.clone())
}

#[tauri::command]
fn set_avatar(appearance: AvatarAppearance, app: AppHandle) -> Result<AvatarAppearance, String> {
    let avatar_clone;
    {
        let state_wrapper = app.state::<GameStateWrapper>();
        let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
        state.avatar = appearance;
        avatar_clone = state.avatar.clone();
    }
    // Broadcast avatar change to peers (GameState lock dropped first)
    let net = app.state::<NetworkWrapper>();
    let mut ns = net.0.lock().unwrap_or_else(|e| e.into_inner());
    let actions = ns.publish_avatar_update(&avatar_clone, &mut rand::rngs::OsRng);
    drop(ns);
    execute_network_actions(&app, actions);
    Ok(avatar_clone)
}

/// Save the current game state to disk. Non-fatal on failure.
/// Locks GameStateWrapper internally — callers must NOT hold that lock.
fn save_current_state(app: &AppHandle) {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = match state_wrapper.0.lock() {
        Ok(s) => s,
        Err(_) => return,
    };
    let save = match state.save_state() {
        Some(s) => s,
        None => return,
    };
    drop(state); // Release lock before file I/O

    let pi = app.state::<PlayerIdentityWrapper>();
    let save_path = pi.data_dir.join("savegame.json");
    if let Err(e) = crate::engine::state::write_save_state(&save_path, &save) {
        eprintln!("[persistence] Save failed: {e}");
    }
}

#[tauri::command]
fn load_street(
    name: String,
    save_state: Option<serde_json::Value>,
    app: AppHandle,
) -> Result<StreetData, String> {
    // Load XML from bundled assets
    let xml = load_street_xml(&name)?;
    let street_data = parse_street(&xml)?;
    let entity_json = load_entity_placement(&name)?;
    let placement = item::loader::parse_entity_placements(&entity_json)?;

    // Save current state BEFORE acquiring GameStateWrapper lock.
    save_current_state(&app);

    // Update game state
    {
        let state_wrapper = app.state::<GameStateWrapper>();
        let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
        state.load_street(
            street_data.clone(),
            placement.entities,
            placement.ground_items,
        );
        // Restore saved position/inventory if provided (auto-resume).
        if let Some(ref save_json) = save_state {
            match serde_json::from_value::<crate::engine::state::SaveState>(save_json.clone()) {
                Ok(save) => state.restore_save(&save),
                Err(e) => eprintln!("[persistence] Failed to deserialize save_state in load_street: {e}"),
            }
        }

        // Recover from an incomplete trade (crash between execute and save).
        let piw = app.state::<PlayerIdentityWrapper>();
        let journal_path = piw.data_dir.join("trade_journal.json");
        if let Some(journal) = trade::journal::read_journal(&journal_path) {
            let already_saved = state
                .last_trade_id
                .is_some_and(|id| id == journal.trade_id);
            if !already_saved {
                state.recover_trade_journal(&journal);
                // Save immediately so recovery is durable.
                // Do NOT clear the journal until the save succeeds — if
                // the disk write fails, we need the journal for the next
                // restart attempt.
                let saved = state.save_state().is_some_and(|save| {
                    let save_path = piw.data_dir.join("savegame.json");
                    match engine::state::write_save_state(&save_path, &save) {
                        Ok(()) => true,
                        Err(e) => {
                            eprintln!("[journal] Failed to save after recovery: {e}");
                            false
                        }
                    }
                });
                if !saved {
                    eprintln!("[journal] Retaining journal for next restart");
                    // Skip clearing — journal stays on disk for retry.
                } else {
                    trade::journal::clear_journal(&journal_path);
                }
            } else {
                // Trade already reflected in save — safe to clean up.
                trade::journal::clear_journal(&journal_path);
            }
        }
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
    // Save game state before tearing down.
    save_current_state(&app);

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
        net_state.send_chat(message, &mut rand::rngs::OsRng)
    };
    execute_network_actions(&app, actions);
    Ok(())
}

#[tauri::command]
fn drop_item(slot: usize, app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    if let Some(stack) = state.inventory.drop_item(slot) {
        let id = format!("drop_{}", state.next_item_id);
        let x = state.player.x;
        let y = state.player.y;
        state.world_items.push(item::types::WorldItem {
            id,
            item_id: stack.item_id,
            count: stack.count,
            x,
            y,
        });
        state.next_item_id += 1;
    }
    Ok(())
}

#[tauri::command]
fn get_recipes(app: AppHandle) -> Result<Vec<item::types::RecipeDef>, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    let mut recipes: Vec<_> = state.recipe_defs.values().cloned().collect();
    recipes.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(recipes)
}

#[tauri::command]
fn craft_recipe(recipe_id: String, app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    state.craft_recipe(&recipe_id).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn street_transition_ready(generation: u64, app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    state.transition.mark_street_ready(generation);
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
            NetworkAction::PresenceChange(ref event) => {
                // Cancel trade only if the departing peer is specifically our trade partner.
                if let network::types::PresenceEvent::Left { address_hash } = event {
                    let trade = app.state::<TradeWrapper>();
                    let mut trade_mgr = trade.0.lock().unwrap_or_else(|e| e.into_inner());
                    let result = trade_mgr.cancel_trade_with_peer(address_hash);
                    drop(trade_mgr);
                    if let Some(ref cancel_msg) = result.cancel_msg {
                        send_trade_msg(app, cancel_msg, address_hash);
                    }
                    if result.cancel_msg.is_some() || result.pending_cleared {
                        let _ = app.emit(
                            "trade_event",
                            serde_json::json!({"type": "cancelled", "reason": "peerDisconnected"}),
                        );
                    }
                }
            }
            NetworkAction::RemotePlayerUpdate { .. } => {}
            NetworkAction::TradeMessageReceived { sender, message } => {
                handle_trade_message(app, sender, message);
            }
        }
    }
}

/// Process an inbound trade protocol message.
/// `authenticated_sender` is the peer's address hash from the authenticated session layer.
fn handle_trade_message(
    app: &AppHandle,
    authenticated_sender: [u8; 16],
    msg: trade::types::TradeMessage,
) {
    use trade::types::TradeMessage;

    // Verify the message's claimed sender matches the authenticated session peer.
    let claimed_sender = match &msg {
        TradeMessage::Request { initiator, .. } => *initiator,
        TradeMessage::Accept { responder, .. } => *responder,
        TradeMessage::Decline { responder, .. } => *responder,
        TradeMessage::Update { sender, .. }
        | TradeMessage::Lock { sender, .. }
        | TradeMessage::Unlock { sender, .. }
        | TradeMessage::Cancel { sender, .. }
        | TradeMessage::Complete { sender, .. } => *sender,
    };
    if claimed_sender != authenticated_sender {
        return; // spoofed message — discard
    }

    let trade = app.state::<TradeWrapper>();
    let mut trade_mgr = trade.0.lock().unwrap_or_else(|e| e.into_inner());

    match msg {
        TradeMessage::Request {
            trade_id,
            initiator,
            recipient,
        } => {
            // Only process if we are the recipient.
            let our_hash = {
                let net = app.state::<NetworkWrapper>();
                let ns = net.0.lock().unwrap_or_else(|e| e.into_inner());
                ns.our_address_hash()
            };
            if recipient != our_hash {
                return;
            }
            let initiator_name = {
                let net = app.state::<NetworkWrapper>();
                let ns = net.0.lock().unwrap_or_else(|e| e.into_inner());
                ns.peer_display_name(&initiator)
                    .unwrap_or_else(|| "Unknown".into())
            };
            if trade_mgr
                .receive_request(trade_id, initiator, initiator_name.clone(), now_secs(app))
                .is_ok()
            {
                let _ = app.emit(
                    "trade_event",
                    serde_json::json!({
                        "type": "request",
                        "tradeId": trade_id,
                        "initiatorHash": hex::encode(initiator),
                        "initiatorName": initiator_name,
                    }),
                );
            }
        }
        TradeMessage::Accept { trade_id, .. } => {
            if trade_mgr.receive_accept(trade_id, &authenticated_sender, now_secs(app)).is_ok() {
                let _ = app.emit(
                    "trade_event",
                    serde_json::json!({"type": "accepted"}),
                );
            }
        }
        TradeMessage::Decline { trade_id, .. } => {
            if trade_mgr.receive_decline(trade_id, &authenticated_sender).is_ok() {
                let _ = app.emit(
                    "trade_event",
                    serde_json::json!({"type": "declined"}),
                );
            }
        }
        TradeMessage::Update {
            trade_id, offer, ..
        } => {
            if trade_mgr
                .receive_remote_update(trade_id, &authenticated_sender, offer, now_secs(app))
                .is_ok()
            {
                let frame = {
                    let state_wrapper = app.state::<GameStateWrapper>();
                    let state = state_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());
                    trade_mgr.trade_frame(&state.item_defs)
                };
                if let Some(frame) = frame {
                    let _ = app.emit(
                        "trade_event",
                        serde_json::json!({"type": "updated", "tradeFrame": frame}),
                    );
                }
            }
        }
        TradeMessage::Lock {
            trade_id,
            terms_hash,
            ..
        } => {
            if let Ok(both_locked) =
                trade_mgr.receive_remote_lock(trade_id, &authenticated_sender, terms_hash, now_secs(app))
            {
                if both_locked {
                    // Both locked with matching hash — execute trade.
                    // Write journal BEFORE execution so a crash can be recovered.
                    // If the journal write fails, abort — proceeding without a
                    // crash-recovery record would silently defeat the safety guarantee.
                    let piw = app.state::<PlayerIdentityWrapper>();
                    let journal_path = piw.data_dir.join("trade_journal.json");
                    let journal_ok = trade_mgr.build_journal().is_some_and(|journal| {
                        trade::journal::write_journal(&journal_path, &journal).is_ok()
                    });
                    if !journal_ok {
                        eprintln!("[trade] journal write failed — aborting trade");
                        let cancel_msg = trade_mgr.cancel_trade();
                        drop(trade_mgr);
                        if let Some(cancel_msg) = cancel_msg {
                            send_trade_msg(app, &cancel_msg, &authenticated_sender);
                        }
                        let _ = app.emit(
                            "trade_event",
                            serde_json::json!({"type": "cancelled", "reason": "Journal write failed"}),
                        );
                        return;
                    }
                    let state_wrapper = app.state::<GameStateWrapper>();
                    let mut guard =
                        state_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());
                    let state = &mut *guard;
                    match trade_mgr
                        .execute_trade(&mut state.inventory, &mut state.currants, &state.item_defs)
                    {
                        Ok(complete_msg) => {
                            // Save state immediately after trade execution.
                            guard.last_trade_id = Some(trade_id);
                            guard.flush_active_craft();
                            let saved = guard.save_state().is_some_and(|save| {
                                let save_path = piw.data_dir.join("savegame.json");
                                engine::state::write_save_state(&save_path, &save).is_ok()
                            });
                            if saved {
                                trade::journal::clear_journal(&journal_path);
                            } else {
                                eprintln!("[trade] Retaining journal — save failed, trade recoverable on restart");
                            }
                            drop(guard);
                            let _ = app.emit(
                                "trade_event",
                                serde_json::json!({"type": "completed"}),
                            );
                            // Send Complete courtesy message to peer.
                            let net = app.state::<NetworkWrapper>();
                            let mut ns =
                                net.0.lock().unwrap_or_else(|e| e.into_inner());
                            let actions =
                                ns.send_trade_message(&complete_msg, &authenticated_sender, &mut rand::rngs::OsRng);
                            drop(ns);
                            drop(trade_mgr);
                            execute_network_actions(app, actions);
                        }
                        Err(e) => {
                            eprintln!("[trade] execution failed: {e}");
                            trade::journal::clear_journal(&journal_path);
                            let cancel_msg = trade_mgr.cancel_trade();
                            drop(guard);
                            drop(trade_mgr);
                            if let Some(cancel_msg) = cancel_msg {
                                send_trade_msg(app, &cancel_msg, &authenticated_sender);
                            }
                            let _ = app.emit(
                                "trade_event",
                                serde_json::json!({"type": "cancelled", "reason": e}),
                            );
                        }
                    }
                } else {
                    let _ = app.emit(
                        "trade_event",
                        serde_json::json!({"type": "locked", "who": "remote"}),
                    );
                }
            }
        }
        TradeMessage::Unlock { trade_id, .. } => {
            if trade_mgr.receive_remote_unlock(trade_id, &authenticated_sender, now_secs(app)).is_ok() {
                let _ = app.emit(
                    "trade_event",
                    serde_json::json!({"type": "unlocked", "who": "remote"}),
                );
            }
        }
        TradeMessage::Cancel { trade_id, .. } => {
            if trade_mgr.receive_cancel(trade_id, &authenticated_sender).is_ok() {
                let _ = app.emit(
                    "trade_event",
                    serde_json::json!({"type": "cancelled", "reason": "peerCancelled"}),
                );
            }
        }
        TradeMessage::Complete { trade_id, .. } => {
            let _ = trade_mgr.receive_complete(trade_id, &authenticated_sender);
        }
    }
}

fn now_secs(app: &AppHandle) -> f64 {
    let epoch = app.state::<MonotonicEpoch>();
    Instant::now().duration_since(epoch.0).as_secs_f64()
}

/// Validate that the player is within interact_radius of the given entity.
fn validate_entity_proximity(state: &engine::state::GameState, entity_id: &str) -> Result<(), String> {
    let entity = state.world_entities.iter().find(|e| e.id == entity_id)
        .ok_or_else(|| format!("Unknown entity: {entity_id}"))?;
    let def = state.entity_defs.get(&entity.entity_type);
    let radius = def.map(|d| d.interact_radius).unwrap_or(60.0);
    let dx = state.player.x - entity.x;
    let dy = state.player.y - entity.y;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist > radius { return Err("Too far".to_string()); }
    Ok(())
}

#[tauri::command]
fn jukebox_play(entity_id: String, app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    validate_entity_proximity(&state, &entity_id)?;
    if let Some(jb) = state.jukebox_states.get_mut(&entity_id) { jb.play(); }
    Ok(())
}

#[tauri::command]
fn jukebox_pause(entity_id: String, app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    validate_entity_proximity(&state, &entity_id)?;
    if let Some(jb) = state.jukebox_states.get_mut(&entity_id) { jb.pause(); }
    Ok(())
}

#[tauri::command]
fn jukebox_select_track(entity_id: String, track_index: usize, app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    validate_entity_proximity(&state, &entity_id)?;
    if let Some(jb) = state.jukebox_states.get_mut(&entity_id) { jb.select_track(track_index); }
    Ok(())
}

#[tauri::command]
fn get_jukebox_state(entity_id: String, app: AppHandle) -> Result<serde_json::Value, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    let entity = state.world_entities.iter().find(|e| e.id == entity_id)
        .ok_or_else(|| format!("Unknown entity: {entity_id}"))?;
    let def = state.entity_defs.get(&entity.entity_type);
    let name = def.map(|d| d.name.as_str()).unwrap_or("Jukebox");

    let jb = state.jukebox_states.get(&entity_id);
    let playlist: Vec<serde_json::Value> = jb
        .map(|jb| {
            jb.playlist.iter().map(|track_id| {
                let track_def = state.track_catalog.tracks.get(track_id);
                serde_json::json!({
                    "id": track_id,
                    "title": track_def.map(|t| t.title.as_str()).unwrap_or("Unknown"),
                    "artist": track_def.map(|t| t.artist.as_str()).unwrap_or("Unknown"),
                    "durationSecs": track_def.map(|t| t.duration_secs).unwrap_or(0.0),
                })
            }).collect()
        })
        .unwrap_or_default();

    Ok(serde_json::json!({
        "entityId": entity_id,
        "name": name,
        "playlist": playlist,
        "currentTrackIndex": jb.map(|j| j.current_track_index).unwrap_or(0),
        "playing": jb.map(|j| j.playing).unwrap_or(false),
        "elapsedSecs": jb.map(|j| j.elapsed_secs).unwrap_or(0.0),
    }))
}

#[tauri::command]
fn get_store_state(entity_id: String, app: AppHandle) -> Result<serde_json::Value, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    // No proximity check — this is a read-only query used to refresh
    // the shop panel after buy/sell (which already validate proximity).
    let entity = state.world_entities.iter().find(|e| e.id == entity_id)
        .ok_or_else(|| format!("Unknown entity: {entity_id}"))?;
    let def = state.entity_defs.get(&entity.entity_type)
        .ok_or_else(|| format!("Unknown entity type: {}", entity.entity_type))?;
    let store_id = def.store.as_ref()
        .ok_or_else(|| format!("Entity '{}' is not a vendor", entity.entity_type))?;
    let store = state.store_catalog.stores.get(store_id)
        .ok_or_else(|| format!("Unknown store: {store_id}"))?;

    let vendor_inventory: Vec<serde_json::Value> = store.inventory.iter().filter_map(|item_id| {
        let item_def = state.item_defs.get(item_id)?;
        let base_cost = item_def.base_cost?;
        Some(serde_json::json!({
            "itemId": item_id,
            "name": item_def.name,
            "baseCost": base_cost,
            "stackLimit": item_def.stack_limit,
        }))
    }).collect();

    // Build player sellable inventory: deduplicate by item_id
    let mut seen = std::collections::HashMap::<String, u32>::new();
    for stack in state.inventory.slots.iter().flatten() {
        *seen.entry(stack.item_id.clone()).or_insert(0) += stack.count;
    }
    let mut player_inventory: Vec<serde_json::Value> = seen.iter().filter_map(|(item_id, &count)| {
        let sell = item::vendor::sell_price(item_id, &state.item_defs, store)?;
        let item_def = state.item_defs.get(item_id)?;
        Some(serde_json::json!({
            "itemId": item_id,
            "name": item_def.name,
            "count": count,
            "sellPrice": sell,
        }))
    }).collect();
    // Sort by item name for stable display order across refreshes
    player_inventory.sort_by(|a, b| {
        a["name"].as_str().unwrap_or("").cmp(b["name"].as_str().unwrap_or(""))
    });

    Ok(serde_json::json!({
        "entityId": entity_id,
        "name": store.name,
        "vendorInventory": vendor_inventory,
        "playerInventory": player_inventory,
        "currants": state.currants,
    }))
}

#[tauri::command]
fn vendor_buy(entity_id: String, item_id: String, count: u32, app: AppHandle) -> Result<u64, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    validate_entity_proximity(&state, &entity_id)?;

    let entity = state.world_entities.iter().find(|e| e.id == entity_id)
        .ok_or_else(|| format!("Unknown entity: {entity_id}"))?;
    let def = state.entity_defs.get(&entity.entity_type)
        .ok_or_else(|| format!("Unknown entity type: {}", entity.entity_type))?;
    let store_id = def.store.as_ref()
        .ok_or_else(|| format!("Entity '{}' is not a vendor", entity.entity_type))?;
    let store = state.store_catalog.stores.get(store_id)
        .ok_or_else(|| format!("Unknown store: {store_id}"))?
        .clone();
    let item_defs = state.item_defs.clone();

    let currants = state.currants;
    let new_balance = item::vendor::buy(&item_id, count, currants, &mut state.inventory, &item_defs, &store)?;
    state.currants = new_balance;

    let total = currants - new_balance;
    let px = state.player.x;
    let py = state.player.y;
    let fb_id = state.next_feedback_id;
    state.next_feedback_id += 1;
    state.pickup_feedback.push(item::types::PickupFeedback {
        id: fb_id,
        text: format!("-{total} currants"),
        success: true,
        x: px,
        y: py,
        age_secs: 0.0,
    });

    Ok(new_balance)
}

#[tauri::command]
fn vendor_sell(entity_id: String, item_id: String, count: u32, app: AppHandle) -> Result<u64, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    validate_entity_proximity(&state, &entity_id)?;

    let entity = state.world_entities.iter().find(|e| e.id == entity_id)
        .ok_or_else(|| format!("Unknown entity: {entity_id}"))?;
    let def = state.entity_defs.get(&entity.entity_type)
        .ok_or_else(|| format!("Unknown entity type: {}", entity.entity_type))?;
    let store_id = def.store.as_ref()
        .ok_or_else(|| format!("Entity '{}' is not a vendor", entity.entity_type))?;
    let store = state.store_catalog.stores.get(store_id)
        .ok_or_else(|| format!("Unknown store: {store_id}"))?
        .clone();
    let item_defs = state.item_defs.clone();

    let old_balance = state.currants;
    let new_balance = item::vendor::sell(&item_id, count, old_balance, &mut state.inventory, &item_defs, &store)?;
    state.currants = new_balance;

    let total = new_balance - old_balance;
    let px = state.player.x;
    let py = state.player.y;
    let fb_id = state.next_feedback_id;
    state.next_feedback_id += 1;
    state.pickup_feedback.push(item::types::PickupFeedback {
        id: fb_id,
        text: format!("+{total} currants"),
        success: true,
        x: px,
        y: py,
        age_secs: 0.0,
    });

    Ok(new_balance)
}

#[tauri::command]
fn eat_item(item_id: String, app: AppHandle) -> Result<serde_json::Value, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    let item_defs = state.item_defs.clone();
    let energy = state.energy;
    let max_energy = state.max_energy;

    let (new_energy, new_max) = item::energy::eat(&item_id, energy, max_energy, &mut state.inventory, &item_defs)?;
    state.energy = new_energy;

    let gained = new_energy - energy;
    let px = state.player.x;
    let py = state.player.y;
    let fb_id = state.next_feedback_id;
    state.next_feedback_id += 1;
    state.pickup_feedback.push(item::types::PickupFeedback {
        id: fb_id,
        text: format!("+{} energy", gained.round() as u32),
        success: true,
        x: px,
        y: py,
        age_secs: 0.0,
    });

    Ok(serde_json::json!({
        "energy": new_energy,
        "maxEnergy": new_max,
    }))
}

// ── Trade IPC commands ──────────────────────────────────────────────────

#[tauri::command]
fn trade_initiate(peer_hash: String, app: AppHandle) -> Result<(), String> {
    let peer_bytes: [u8; 16] = hex::decode(&peer_hash)
        .map_err(|_| "Invalid peer hash".to_string())?
        .try_into()
        .map_err(|_| "Peer hash must be 16 bytes".to_string())?;

    let peer_name = {
        let net = app.state::<NetworkWrapper>();
        let ns = net.0.lock().map_err(|e| e.to_string())?;
        ns.peer_display_name(&peer_bytes)
            .unwrap_or_else(|| "Unknown".into())
    };

    let trade_id: u64 = rand::random();
    let msg = {
        let trade = app.state::<TradeWrapper>();
        let mut mgr = trade.0.lock().map_err(|e| e.to_string())?;
        mgr.initiate_trade(trade_id, peer_bytes, peer_name, now_secs(&app))?
    };
    send_trade_msg(&app, &msg, &peer_bytes);
    Ok(())
}

#[tauri::command]
fn trade_accept(app: AppHandle) -> Result<(), String> {
    let (msg, peer_hash) = {
        let trade = app.state::<TradeWrapper>();
        let mut mgr = trade.0.lock().map_err(|e| e.to_string())?;
        let msg = mgr.accept_trade(now_secs(&app))?;
        let peer = mgr.active_peer_hash().ok_or("No active trade after accept")?;
        (msg, peer)
    };
    send_trade_msg(&app, &msg, &peer_hash);
    // Emit accepted event so the responder's own UI transitions from prompt to trade panel.
    let _ = app.emit(
        "trade_event",
        serde_json::json!({"type": "accepted"}),
    );
    Ok(())
}

#[tauri::command]
fn trade_decline(app: AppHandle) -> Result<(), String> {
    let (msg, peer_hash) = {
        let trade = app.state::<TradeWrapper>();
        let mut mgr = trade.0.lock().map_err(|e| e.to_string())?;
        let peer = mgr.pending_peer_hash().ok_or("No pending trade to decline")?;
        let msg = mgr.decline_trade()?;
        (msg, peer)
    };
    send_trade_msg(&app, &msg, &peer_hash);
    Ok(())
}

#[tauri::command]
fn trade_update_offer(
    items: Vec<item::types::ItemStack>,
    currants: u64,
    app: AppHandle,
) -> Result<(), String> {
    let offer = trade::types::TradeOffer { items, currants };
    let (msg, peer_hash) = {
        let trade = app.state::<TradeWrapper>();
        let mut mgr = trade.0.lock().map_err(|e| e.to_string())?;
        let peer = mgr.active_peer_hash().ok_or("No active trade")?;
        let msg = mgr.update_offer(offer, now_secs(&app))?;
        (msg, peer)
    };
    send_trade_msg(&app, &msg, &peer_hash);
    Ok(())
}

#[tauri::command]
fn trade_lock(app: AppHandle) -> Result<(), String> {
    // Hold the trade lock continuously from lock_trade() through execute_trade()
    // to prevent a race where an inbound Complete/Cancel clears the trade between
    // the two operations.
    let trade = app.state::<TradeWrapper>();
    let mut mgr = trade.0.lock().map_err(|e| e.to_string())?;
    let peer_hash = mgr.active_peer_hash().ok_or("No active trade")?;
    let lock_msg = {
        let state_wrapper = app.state::<GameStateWrapper>();
        let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
        mgr.lock_trade(&state.inventory, state.currants, now_secs(&app))?
    };

    // Check if both are now locked (lock_trade may have set Executing).
    let is_executing = mgr.is_executing();

    if is_executing {
        // Write journal BEFORE execution so a crash can be recovered.
        // If the journal write fails, abort — proceeding without a
        // crash-recovery record would silently defeat the safety guarantee.
        let piw = app.state::<PlayerIdentityWrapper>();
        let journal_path = piw.data_dir.join("trade_journal.json");
        let journal = match mgr.build_journal() {
            Some(j) => j,
            None => {
                eprintln!("[trade] build_journal returned None in Executing phase — aborting");
                let cancel_msg = mgr.cancel_trade();
                drop(mgr);
                if let Some(cancel_msg) = cancel_msg {
                    send_trade_msg(&app, &cancel_msg, &peer_hash);
                }
                return Err("Failed to build trade journal".into());
            }
        };
        if let Err(e) = trade::journal::write_journal(&journal_path, &journal) {
            eprintln!("[trade] journal write failed: {e} — aborting trade");
            let cancel_msg = mgr.cancel_trade();
            drop(mgr);
            if let Some(cancel_msg) = cancel_msg {
                send_trade_msg(&app, &cancel_msg, &peer_hash);
            }
            let _ = app.emit(
                "trade_event",
                serde_json::json!({"type": "cancelled", "reason": "Journal write failed"}),
            );
            return Err("Journal write failed — trade aborted".into());
        }
        let state_wrapper = app.state::<GameStateWrapper>();
        let mut guard = state_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());
        let state = &mut *guard;
        match mgr.execute_trade(&mut state.inventory, &mut state.currants, &state.item_defs) {
            Ok(complete_msg) => {
                guard.last_trade_id = Some(journal.trade_id);
                guard.flush_active_craft();
                let saved = guard.save_state().is_some_and(|save| {
                    let save_path = piw.data_dir.join("savegame.json");
                    engine::state::write_save_state(&save_path, &save).is_ok()
                });
                if saved {
                    trade::journal::clear_journal(&journal_path);
                } else {
                    eprintln!("[trade] Retaining journal — save failed, trade recoverable on restart");
                }
                drop(guard);
                drop(mgr);
                // Defer network sends until after all locks are released.
                send_trade_msg(&app, &lock_msg, &peer_hash);
                send_trade_msg(&app, &complete_msg, &peer_hash);
                let _ = app.emit(
                    "trade_event",
                    serde_json::json!({"type": "completed"}),
                );
            }
            Err(e) => {
                // Do NOT send lock_msg — the peer would see both locked,
                // execute their side, and we'd have a one-sided trade.
                trade::journal::clear_journal(&journal_path);
                let cancel_msg = mgr.cancel_trade();
                drop(guard);
                drop(mgr);
                if let Some(cancel_msg) = cancel_msg {
                    send_trade_msg(&app, &cancel_msg, &peer_hash);
                }
                let _ = app.emit(
                    "trade_event",
                    serde_json::json!({"type": "cancelled", "reason": e}),
                );
            }
        }
    } else {
        drop(mgr);
        send_trade_msg(&app, &lock_msg, &peer_hash);
        let _ = app.emit(
            "trade_event",
            serde_json::json!({"type": "locked", "who": "local"}),
        );
    }
    Ok(())
}

#[tauri::command]
fn trade_unlock(app: AppHandle) -> Result<(), String> {
    let (msg, peer_hash) = {
        let trade = app.state::<TradeWrapper>();
        let mut mgr = trade.0.lock().map_err(|e| e.to_string())?;
        let peer = mgr.active_peer_hash().ok_or("No active trade")?;
        let msg = mgr.unlock_trade(now_secs(&app))?;
        (msg, peer)
    };
    send_trade_msg(&app, &msg, &peer_hash);
    let _ = app.emit(
        "trade_event",
        serde_json::json!({"type": "unlocked", "who": "local"}),
    );
    Ok(())
}

#[tauri::command]
fn trade_cancel(app: AppHandle) -> Result<(), String> {
    let trade = app.state::<TradeWrapper>();
    let mut mgr = trade.0.lock().map_err(|e| e.to_string())?;
    let peer_hash = mgr.active_peer_hash();
    if let (Some(msg), Some(peer)) = (mgr.cancel_trade(), peer_hash) {
        drop(mgr);
        send_trade_msg(&app, &msg, &peer);
    }
    let _ = app.emit(
        "trade_event",
        serde_json::json!({"type": "cancelled", "reason": "localCancelled"}),
    );
    Ok(())
}

#[tauri::command]
fn trade_get_state(app: AppHandle) -> Result<Option<trade::types::TradeFrame>, String> {
    let trade = app.state::<TradeWrapper>();
    let mgr = trade.0.lock().map_err(|e| e.to_string())?;
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    Ok(mgr.trade_frame(&state.item_defs))
}

/// Helper to send a trade message via the network.
fn send_trade_msg(app: &AppHandle, msg: &trade::types::TradeMessage, target: &[u8; 16]) {
    let net = app.state::<NetworkWrapper>();
    let mut ns = net.0.lock().unwrap_or_else(|e| e.into_inner());
    let actions = ns.send_trade_message(msg, target, &mut rand::rngs::OsRng);
    drop(ns);
    execute_network_actions(app, actions);
}

fn game_loop(app: AppHandle) {
    let tick_duration = Duration::from_secs_f64(1.0 / 60.0);
    let dt = 1.0 / 60.0;
    let game_start = app.state::<MonotonicEpoch>().0;
    let mut rng = rand::rngs::ThreadRng::default();
    let mut tick_count: u64 = 0;

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

        // 4. Execute NetworkActions (broadcast packets, trade messages, presence)
        execute_network_actions(&app, net_actions);

        // 4b. Tick trade manager for timeouts.
        {
            let trade = app.state::<TradeWrapper>();
            let mut trade_mgr = trade.0.lock().unwrap_or_else(|e| e.into_inner());
            // Capture peer hash before tick() clears the trade.
            let active_peer = trade_mgr.active_peer_hash();
            let tick_result = trade_mgr.tick(now_secs);
            drop(trade_mgr);
            if let (Some(cancel_msg), Some(peer)) = (tick_result.cancel_msg, active_peer) {
                let net = app.state::<NetworkWrapper>();
                let mut ns = net.0.lock().unwrap_or_else(|e| e.into_inner());
                let actions = ns.send_trade_message(&cancel_msg, &peer, &mut rng);
                drop(ns);
                execute_network_actions(&app, actions);
                let _ = app.emit(
                    "trade_event",
                    serde_json::json!({"type": "cancelled", "reason": "timeout"}),
                );
            }
            if tick_result.pending_expired {
                let _ = app.emit(
                    "trade_event",
                    serde_json::json!({"type": "cancelled", "reason": "timeout"}),
                );
            }
        }

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
            state.tick(dt, &input, &mut rng)
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
                    animation: match frame.player.animation {
                        AnimationState::Idle => 0,
                        AnimationState::Walking => 1,
                        AnimationState::Jumping => 2,
                        AnimationState::Falling => 3,
                    },
                };
                let net = app.state::<NetworkWrapper>();
                let mut ns = net.0.lock().unwrap_or_else(|e| e.into_inner());
                ns.publish_player_state(&net_state, &mut rand::rngs::OsRng)
            };
            execute_network_actions(&app, publish_actions);

            // 8b. Periodically broadcast avatar to peers (every 5s = 300 ticks).
            // Ensures newly connected peers receive our appearance.
            if tick_count.is_multiple_of(300) {
                let avatar = {
                    let state_wrapper = app.state::<GameStateWrapper>();
                    let state = state_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());
                    state.avatar.clone()
                };
                let net = app.state::<NetworkWrapper>();
                let mut ns = net.0.lock().unwrap_or_else(|e| e.into_inner());
                let actions = ns.publish_avatar_update(&avatar, &mut rand::rngs::OsRng);
                drop(ns);
                execute_network_actions(&app, actions);
            }
            tick_count += 1;

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

fn load_entity_placement(name: &str) -> Result<String, String> {
    match name {
        "demo_meadow" | "LADEMO001" => {
            Ok(include_str!("../../assets/streets/demo_meadow_entities.json").to_string())
        }
        "demo_heights" | "LADEMO002" => {
            Ok(include_str!("../../assets/streets/demo_heights_entities.json").to_string())
        }
        _ => Ok("[]".to_string()), // Streets without entities get an empty list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_kit_id_accepts_valid() {
        assert!(validate_kit_id("retro-kit").is_ok());
        assert!(validate_kit_id("my_kit_2").is_ok());
        assert!(validate_kit_id("Default").is_ok());
    }

    #[test]
    fn validate_kit_id_rejects_path_traversal() {
        assert!(validate_kit_id("..").is_err());
        assert!(validate_kit_id("../etc").is_err());
        assert!(validate_kit_id("foo/bar").is_err());
        assert!(validate_kit_id("foo\\bar").is_err());
    }

    #[test]
    fn validate_kit_id_rejects_empty() {
        assert!(validate_kit_id("").is_err());
    }

    #[test]
    fn validate_kit_id_rejects_dots() {
        assert!(validate_kit_id("my.kit").is_err());
    }

    #[test]
    fn read_default_kit_parses() {
        let json: serde_json::Value =
            serde_json::from_str(include_str!("../../assets/audio/default-kit.json"))
                .expect("bundled default-kit.json must be valid JSON");
        assert_eq!(json["name"], "Default");
        assert!(json["events"]["jump"]["default"].is_string());
    }
}

pub fn run() {
    tauri::Builder::default()
        .register_uri_scheme_protocol("soundkit", |ctx, request| -> http::Response<Vec<u8>> {
            let app = ctx.app_handle();
            let kits_dir = app.state::<SoundKitsDir>().0.clone();

            let uri_path = request.uri().path();
            let trimmed = uri_path.trim_start_matches('/');
            let (kit_id, file_path) = match trimmed.split_once('/') {
                Some((k, f)) => (k, f),
                None => {
                    return http::Response::builder()
                        .status(400)
                        .body(b"Invalid path".to_vec())
                        .unwrap();
                }
            };

            if validate_kit_id(kit_id).is_err() {
                return http::Response::builder()
                    .status(403)
                    .body(b"Invalid kit ID".to_vec())
                    .unwrap();
            }

            // Percent-decode the file path so filenames with spaces/special chars work
            let decoded_path = percent_encoding::percent_decode_str(file_path)
                .decode_utf8()
                .unwrap_or_default();

            if decoded_path.contains("..") {
                return http::Response::builder()
                    .status(403)
                    .body(b"Path traversal rejected".to_vec())
                    .unwrap();
            }

            let full_path = kits_dir.join(kit_id).join(decoded_path.as_ref());

            let canonical_kits = match kits_dir.canonicalize() {
                Ok(p) => p,
                Err(_) => {
                    return http::Response::builder()
                        .status(404)
                        .body(b"Kits directory not found".to_vec())
                        .unwrap();
                }
            };
            let canonical_file = match full_path.canonicalize() {
                Ok(p) => p,
                Err(_) => {
                    return http::Response::builder()
                        .status(404)
                        .body(b"File not found".to_vec())
                        .unwrap();
                }
            };
            if !canonical_file.starts_with(&canonical_kits) {
                return http::Response::builder()
                    .status(403)
                    .body(b"Access denied".to_vec())
                    .unwrap();
            }

            let bytes = match std::fs::read(&canonical_file) {
                Ok(b) => b,
                Err(_) => {
                    return http::Response::builder()
                        .status(404)
                        .body(b"File not found".to_vec())
                        .unwrap();
                }
            };

            let mime = match canonical_file.extension().and_then(|e| e.to_str()) {
                Some("mp3") => "audio/mpeg",
                Some("ogg") => "audio/ogg",
                Some("wav") => "audio/wav",
                Some("flac") => "audio/flac",
                _ => "application/octet-stream",
            };

            http::Response::builder()
                .status(200)
                .header("Content-Type", mime)
                .header("Access-Control-Allow-Origin", "*")
                .body(bytes)
                .unwrap()
        })
        .manage(MonotonicEpoch(Instant::now()))
        .manage({
            let item_defs = item::loader::parse_item_defs(include_str!("../../assets/items.json"))
                .expect("Failed to parse items.json");
            let entity_defs =
                item::loader::parse_entity_defs(include_str!("../../assets/entities.json"))
                    .expect("Failed to parse entities.json");
            let recipe_defs =
                item::loader::parse_recipe_defs(include_str!("../../assets/recipes.json"))
                    .expect("Failed to parse recipes.json");
            // Validate all recipe item references and reject duplicate entries
            for (recipe_id, recipe) in &recipe_defs {
                let mut seen_inputs = std::collections::HashSet::new();
                for input in &recipe.inputs {
                    assert!(
                        item_defs.contains_key(&input.item),
                        "Recipe '{recipe_id}' references unknown input item '{}'",
                        input.item
                    );
                    assert!(
                        seen_inputs.insert(&input.item),
                        "Recipe '{recipe_id}' has duplicate input item '{}'",
                        input.item
                    );
                }
                let mut seen_tools = std::collections::HashSet::new();
                for tool in &recipe.tools {
                    assert!(
                        item_defs.contains_key(&tool.item),
                        "Recipe '{recipe_id}' references unknown tool item '{}'",
                        tool.item
                    );
                    assert!(
                        seen_tools.insert(&tool.item),
                        "Recipe '{recipe_id}' has duplicate tool item '{}'",
                        tool.item
                    );
                }
                let mut seen_outputs = std::collections::HashSet::new();
                for output in &recipe.outputs {
                    assert!(
                        item_defs.contains_key(&output.item),
                        "Recipe '{recipe_id}' references unknown output item '{}'",
                        output.item
                    );
                    assert!(
                        seen_outputs.insert(&output.item),
                        "Recipe '{recipe_id}' has duplicate output item '{}'",
                        output.item
                    );
                }
            }
            let track_catalog = engine::jukebox::parse_catalog(
                include_str!("../../assets/music/catalog.json")
            ).unwrap_or_else(|e| {
                eprintln!("[jukebox] Failed to load music catalog: {e}");
                engine::jukebox::TrackCatalog { tracks: std::collections::HashMap::new() }
            });
            let store_catalog = item::loader::parse_store_catalog(
                include_str!("../../assets/stores.json")
            ).unwrap_or_else(|e| {
                eprintln!("[economy] Failed to load stores.json: {e}");
                item::types::StoreCatalog { stores: std::collections::HashMap::new() }
            });
            for (store_id, store) in &store_catalog.stores {
                for item_id in &store.inventory {
                    assert!(
                        item_defs.contains_key(item_id),
                        "Store '{store_id}' references unknown item '{item_id}'"
                    );
                }
            }
            GameStateWrapper(Mutex::new(GameState::new(
                1280.0,
                720.0,
                item_defs,
                entity_defs,
                recipe_defs,
                track_catalog,
                store_catalog,
            )))
        })
        .manage(InputStateWrapper(Mutex::new(InputState::default())))
        .manage(GameRunning(Mutex::new(false)))
        .manage(GameLoopHandle(Mutex::new(None)))
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let kits_dir = data_dir.join("sound-kits");
            if let Err(e) = std::fs::create_dir_all(&kits_dir) {
                eprintln!("[sound-kits] Failed to create {}: {e}", kits_dir.display());
            }
            app.manage(SoundKitsDir(kits_dir));
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
            let our_hash = net_identity.public_identity().address_hash;
            app.manage(NetworkWrapper(Mutex::new(NetworkState::new(
                net_identity,
                display_name,
            ))));
            app.manage(TradeWrapper(Mutex::new(trade::state::TradeManager::new(our_hash))));

            // Bind UDP transport for LAN discovery.
            let transport = UdpTransport::bind(DEFAULT_PORT)
                .map_err(|e| std::io::Error::other(format!("UDP bind failed: {e}")))?;
            app.manage(TransportWrapper(Mutex::new(transport)));

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                save_current_state(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_streets,
            list_sound_kits,
            read_sound_kit,
            load_street,
            send_input,
            start_game,
            stop_game,
            get_identity,
            set_display_name,
            send_chat,
            drop_item,
            get_recipes,
            craft_recipe,
            street_transition_ready,
            get_network_status,
            get_saved_state,
            jukebox_play,
            jukebox_pause,
            jukebox_select_track,
            get_jukebox_state,
            get_avatar,
            set_avatar,
            get_store_state,
            vendor_buy,
            vendor_sell,
            eat_item,
            trade_initiate,
            trade_accept,
            trade_decline,
            trade_update_offer,
            trade_lock,
            trade_unlock,
            trade_cancel,
            trade_get_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running harmony-glitch");
}
