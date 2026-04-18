pub mod avatar;
pub mod buff;
pub mod date_util;
pub mod emote;
pub mod engine;
pub mod identity;
pub mod item;
pub mod mood;
pub mod network;
pub mod persistence;
pub mod physics;
pub mod quest;
pub mod skill;
pub mod social;
pub mod street;
pub mod trade;
pub mod trust;

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
use tauri::http;
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
    identity_proof: harmony_identity::IdentityProof,
    display_name: Mutex<String>,
    setup_complete: Mutex<bool>,
    data_dir: std::path::PathBuf,
}

/// Path to the sound-kits directory, created on startup.
struct SoundKitsDir(std::path::PathBuf);

/// Path to the imported streets directory. Contains individual XML files
/// and a manifest.json. May not exist if no import has been run.
struct StreetsDir(std::path::PathBuf);

/// Cached street manifest, loaded once at startup.
struct StreetManifestState(street::manifest::StreetManifest);

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

/// Shared group state — manages persistent group DAGs.
struct GroupManagerWrapper(Mutex<crate::social::groups::GroupManager>);

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StreetListEntry {
    tsid: String,
    name: String,
}

#[tauri::command]
fn list_streets(manifest: tauri::State<StreetManifestState>) -> Vec<StreetListEntry> {
    let mut entries = vec![
        StreetListEntry {
            tsid: "LADEMO001".into(),
            name: "Demo Meadow".into(),
        },
        StreetListEntry {
            tsid: "LADEMO002".into(),
            name: "Demo Heights".into(),
        },
    ];
    for (tsid, entry) in &manifest.0.streets {
        // Skip if already covered by embedded demo streets
        if tsid == "LADEMO001" || tsid == "LADEMO002" {
            continue;
        }
        entries.push(StreetListEntry {
            tsid: tsid.clone(),
            name: entry.name.clone(),
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
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
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
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
            if load_street_xml(&s.street_id, &app).is_err() {
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
    // Load XML from bundled assets or imported streets directory
    let xml = load_street_xml(&name, &app)?;
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
                Err(e) => {
                    eprintln!("[persistence] Failed to deserialize save_state in load_street: {e}")
                }
            }
        }

        // Recover from an incomplete trade (crash between execute and save).
        let piw = app.state::<PlayerIdentityWrapper>();
        let journal_path = piw.data_dir.join("trade_journal.json");
        if let Some(journal) = trade::journal::read_journal(&journal_path) {
            let already_saved = state.last_trade_id.is_some_and(|id| id == journal.trade_id);
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
        net_state.set_street_bounds(
            street_data.left,
            street_data.right,
            street_data.top,
            street_data.bottom,
        );
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
            identity_proof: Some(pi.identity_proof),
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
    // Populate skill gating fields for the frontend
    for recipe in &mut recipes {
        if let Some(skill_id) = state.recipe_skill_gate.get(&recipe.id) {
            recipe.required_skill = Some(skill_id.clone());
            recipe.locked = !state.skill_progress.learned.contains(skill_id);
        }
    }
    recipes.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(recipes)
}

#[tauri::command]
fn get_skills(app: AppHandle) -> Result<Vec<skill::types::SkillDef>, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    let mut skills: Vec<_> = state.skill_defs.values().cloned().collect();
    skills.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(skills)
}

#[tauri::command]
fn learn_skill(skill_id: String, app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    state.learn_skill(&skill_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn cancel_learning(app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    state.cancel_skill_learning().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_dialogue_state(
    entity_id: String,
    app: AppHandle,
) -> Result<quest::types::DialogueFrame, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    // Find the entity and its dialogue tree ID
    let entity = state
        .world_entities
        .iter()
        .find(|e| e.id == entity_id)
        .ok_or_else(|| format!("Entity not found: {entity_id}"))?;
    let def = state
        .entity_defs
        .get(&entity.entity_type)
        .ok_or_else(|| format!("Entity def not found: {}", entity.entity_type))?;
    let tree_id = def
        .dialogue
        .as_ref()
        .ok_or_else(|| format!("Entity {} has no dialogue", entity_id))?
        .clone();

    let frame = quest::dialogue::evaluate_start(
        &tree_id,
        &state.dialogue_defs,
        &state.quest_progress,
        &state.quest_defs,
        &state.inventory,
        &state.skill_progress,
        &entity_id,
    )
    .ok_or_else(|| "Dialogue tree or start node not found".to_string())?;

    let start_node = state.dialogue_defs[&tree_id].start_node.clone();
    state.active_dialogue = Some(quest::types::ActiveDialogue {
        tree_id,
        entity_id,
        current_node: start_node,
    });

    Ok(frame)
}

#[tauri::command]
fn dialogue_choose(
    option_index: usize,
    app: AppHandle,
) -> Result<quest::types::DialogueChoiceResult, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    let dialogue = state
        .active_dialogue
        .as_ref()
        .ok_or("No active dialogue")?
        .clone();

    // Destructure to satisfy the borrow checker — evaluate_choice needs
    // both immutable and mutable borrows on different GameState fields.
    let state = &mut *state;
    let result = quest::dialogue::evaluate_choice(
        &dialogue.tree_id,
        &dialogue.current_node,
        option_index,
        &state.dialogue_defs,
        &mut state.quest_progress,
        &state.quest_defs,
        &mut state.inventory,
        &state.skill_progress,
        &mut state.currants,
        &mut state.imagination,
        &state.item_defs,
        &dialogue.entity_id,
    );

    match &result {
        quest::types::DialogueChoiceResult::Continue { next_node_id, .. } => {
            if let Some(ref mut active) = state.active_dialogue {
                active.current_node = next_node_id.clone();
            }
        }
        quest::types::DialogueChoiceResult::End { .. } => {
            state.active_dialogue = None;
        }
    }

    Ok(result)
}

#[tauri::command]
fn close_dialogue(app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    state.active_dialogue = None;
    Ok(())
}

#[tauri::command]
fn get_quest_log(app: AppHandle) -> Result<quest::types::QuestLogFrame, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    let mut active = Vec::new();
    for (quest_id, active_quest) in &state.quest_progress.active {
        if let Some(def) = state.quest_defs.get(quest_id) {
            let objectives = def
                .objectives
                .iter()
                .enumerate()
                .map(|(i, obj)| {
                    let current = active_quest.objective_progress.get(i).copied().unwrap_or(0);
                    let target = obj.target_count();
                    quest::types::ObjectiveEntry {
                        description: obj.description().to_string(),
                        current,
                        target,
                        complete: current >= target,
                    }
                })
                .collect();
            active.push(quest::types::QuestEntry {
                quest_id: quest_id.clone(),
                name: def.name.clone(),
                description: def.description.clone(),
                objectives,
            });
        }
    }
    // Sort active quests by name for stable display
    active.sort_by(|a, b| a.name.cmp(&b.name));

    let mut completed = Vec::new();
    for quest_id in &state.quest_progress.completed {
        if let Some(def) = state.quest_defs.get(quest_id) {
            completed.push(quest::types::QuestCompletedEntry {
                quest_id: quest_id.clone(),
                name: def.name.clone(),
            });
        }
    }

    Ok(quest::types::QuestLogFrame { active, completed })
}

// ── Social: Mood & Emotes ────────────────────────────────────────────────────

/// Result of attempting to fire an emote. Serializable for IPC.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EmoteFireResult {
    /// Emote fired and broadcast. Sender mood may or may not have been
    /// credited (depends on reward cooldown). `cooldown_ms` is the
    /// longest-applicable post-fire cooldown for this kind — lets the UI
    /// dim the button immediately instead of waiting for the next
    /// attempt to be rejected.
    Success { cooldown_ms: u64 },
    /// Cooldown (global or per-pair) blocked this fire. `remaining_ms`
    /// is the time until the emote can next fire.
    Cooldown { remaining_ms: u64 },
    /// Emote requires a target (hug, high_five) and none was provided
    /// or none was in range.
    NoTarget,
    /// Emote was blocked due to target being blocked by this player.
    TargetBlocked,
}

/// Pure inner logic — reusable, no Tauri runtime. Operates on state refs.
///
/// - Applies sender-side cooldown checks (`EmoteFireResult::Cooldown` on fail)
/// - Applies sender mood (gated by reward cooldown)
/// - Marks fire cooldown
/// - Returns the result for the caller to broadcast the EmoteMessage.
///
/// Does NOT emit events or publish to network — caller does that on
/// `EmoteFireResult::Success`.
fn fire_emote(
    emotes: &mut emote::EmoteState,
    mood: &mut crate::mood::MoodState,
    self_identity: [u8; 16],
    kind: &emote::EmoteKind,
    target: Option<[u8; 16]>,
    now: std::time::Instant,
) -> EmoteFireResult {
    // Hug and HighFive physically require a target (intimate gestures with
    // a specific recipient). Wave is dual-mode per spec §1 — "Targeted or
    // broadcast" — so a broadcast wave (casual greeting to the street) is
    // valid; the 30s reward cooldown is what gates sender-mood farming.
    let tag = emote::EmoteKindTag::from(kind);
    if matches!(
        tag,
        emote::EmoteKindTag::Hug | emote::EmoteKindTag::HighFive
    ) && target.is_none()
    {
        return EmoteFireResult::NoTarget;
    }

    // Fire cooldown check
    if let Err(remaining) = emotes.cooldowns.check_fire(now, kind, target) {
        return EmoteFireResult::Cooldown {
            remaining_ms: remaining.remaining_ms,
        };
    }

    // Apply sender mood (gated by reward cooldown).
    // Pair identity for self-rewarded emotes is our own identity;
    // for targeted emotes, the target.
    let sender_delta = sender_mood_delta(kind);
    if sender_delta > 0.0 {
        let pair = target.unwrap_or(self_identity);
        if emotes.cooldowns.try_reward(now, kind, pair) {
            mood.apply_mood_change(sender_delta);
        }
    }

    // Record the fire
    emotes.cooldowns.mark_fire(now, kind, target);

    // Post-fire cooldown — max of global anti-mash and per-pair (if any).
    // The UI uses this to dim the button immediately after firing.
    let global_ms = emote::cooldowns::GLOBAL_FIRE_COOLDOWN.as_millis() as u64;
    let pair_ms = emote::cooldowns::fire_cooldown_for(tag).as_millis() as u64;
    let cooldown_ms = global_ms.max(pair_ms);

    EmoteFireResult::Success { cooldown_ms }
}

/// Pure helper — returns true iff target is within `max_dist` of self.
fn is_target_in_range(self_x: f64, self_y: f64, target_x: f64, target_y: f64, max_dist: f64) -> bool {
    let dx = self_x - target_x;
    let dy = self_y - target_y;
    (dx * dx + dy * dy).sqrt() <= max_dist
}

/// Receive-path target coercion. Mirrors the sender-side hardening so a
/// misbehaving peer can't undermine broadcast-only semantics by wiring a
/// target on a kind that must be broadcast.
///
/// Only Dance is coerced here: its receive-logic awards mood only when
/// `nearby_witness` is true, so a targeted Dance would otherwise be dropped
/// as "not aimed at me" by bystanders. Applaud is dual-nature (targeted
/// OR witness), so its target is preserved — bystanders process it via the
/// Applaud-specific drop-guard exemption below.
fn effective_receive_target(emote: &emote::EmoteMessage) -> Option<[u8; 16]> {
    match emote.kind {
        emote::EmoteKind::Dance => None,
        _ => emote.target,
    }
}

/// Sender-side mood delta per emote kind. See spec §6.
fn sender_mood_delta(kind: &emote::EmoteKind) -> f64 {
    match kind {
        emote::EmoteKind::Hi(_) => 0.0, // Hi sender mood is applied on receive (match bonus)
        emote::EmoteKind::Dance => 2.0,
        emote::EmoteKind::Wave => 1.0,
        emote::EmoteKind::Hug => 5.0,
        emote::EmoteKind::HighFive => 3.0,
        emote::EmoteKind::Applaud => 1.0,
    }
}

/// Receiver-side mood application. Pure, testable. Returns the mood delta
/// that was applied (0.0 if nothing applied).
///
/// - `sender`: the emote's sender (NOT us)
/// - `kind`: the emote kind
/// - `we_are_target`: true iff `emote.target == our_address`
/// - `nearby_witness`: true iff we are within witness radius (300px) of sender
///   AND emote is a broadcast (no target or target != us)
/// - Reward cooldown gates whether mood is actually credited.
///
/// Hi is excluded here — Hi receive-path mood stays in `handle_incoming_hi`
/// (caller must check `EmoteKind::Hi(_)` and route there).
fn apply_receive_mood(
    emotes: &mut emote::EmoteState,
    mood: &mut crate::mood::MoodState,
    sender: [u8; 16],
    kind: &emote::EmoteKind,
    we_are_target: bool,
    nearby_witness: bool,
    now: std::time::Instant,
) -> f64 {
    let delta = match kind {
        emote::EmoteKind::Hi(_) => 0.0, // handled elsewhere
        emote::EmoteKind::Dance => if nearby_witness { 1.0 } else { 0.0 },
        emote::EmoteKind::Wave => if we_are_target { 1.0 } else { 0.0 },
        emote::EmoteKind::Hug => if we_are_target { 5.0 } else { 0.0 },
        emote::EmoteKind::HighFive => if we_are_target { 3.0 } else { 0.0 },
        emote::EmoteKind::Applaud => {
            if we_are_target || nearby_witness { 3.0 } else { 0.0 }
        }
    };

    if delta <= 0.0 {
        return 0.0;
    }

    // Reward-cooldown gate — per (sender, kind) pair
    if !emotes.cooldowns.try_reward(now, kind, sender) {
        return 0.0;
    }

    mood.apply_mood_change(delta);
    delta
}

#[tauri::command]
fn get_mood(app: AppHandle) -> Result<serde_json::Value, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "mood": state.social.mood.mood,
        "maxMood": state.social.mood.max_mood,
        "multiplier": state.social.mood.multiplier(),
    }))
}

#[tauri::command]
fn emote_hi(app: AppHandle) -> Result<serde_json::Value, String> {
    // Resolve identity + nearest target under Net lock
    let (our_address, nearest_hash) = {
        let net = app.state::<NetworkWrapper>();
        let net_state = net.0.lock().map_err(|e| e.to_string())?;
        let our_hash = net_state.our_address_hash();
        let remote_frames = net_state.remote_frames();
        drop(net_state);

        let state_wrapper = app.state::<GameStateWrapper>();
        let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

        let mut nearest: Option<[u8; 16]> = None;
        let mut nearest_dist = f64::MAX;
        for rf in &remote_frames {
            if let Ok(bytes) = hex::decode(&rf.address_hash) {
                if bytes.len() == 16 {
                    let mut addr = [0u8; 16];
                    addr.copy_from_slice(&bytes);
                    let dx = state.player.x - rf.x;
                    let dy = state.player.y - rf.y;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if dist <= 400.0 && dist < nearest_dist {
                        nearest_dist = dist;
                        nearest = Some(addr);
                    }
                }
            }
        }
        (our_hash, nearest)
    };

    // Pre-check Hi daily-per-target gate and block list, but DO NOT record yet —
    // recording is deferred until the unified fire path confirms success. This
    // prevents a cooldown/no-target/blocked rejection from consuming today's
    // Hi allowance for this target.
    let our_variant = {
        let state_wrapper = app.state::<GameStateWrapper>();
        let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
        state.social.set_identity(our_address);

        if let Some(target) = nearest_hash {
            if !state.social.emotes.can_hi(&target) {
                return Err("Already greeted today".to_string());
            }
            if state.social.buddies.is_blocked(&target) {
                return Err("Player is blocked".to_string());
            }
        }

        state.social.emotes.active_variant()
    };

    // Fire + publish via shared helper. emote_hi skips the generic `emote`
    // Tauri command (which rejects Hi) and calls the helper directly —
    // range check was already enforced by our own 400px target selection,
    // and the daily gate pre-check above replaces the generic block check.
    let result = fire_and_publish_emote(
        &app,
        emote::EmoteKind::Hi(our_variant),
        nearest_hash,
        our_address,
    )?;

    match result {
        EmoteFireResult::Success { cooldown_ms } => {
            // Fire succeeded — NOW consume the daily Hi allowance.
            if let Some(target) = nearest_hash {
                let state_wrapper = app.state::<GameStateWrapper>();
                let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
                state.social.emotes.record_hi_sent(target);
            }
            // Return cooldown_ms so the frontend can seed its expiry map
            // immediately — parity with the generic emote IPC, so a
            // follow-up click on the Hi button sees a dimmed state
            // instead of eating a backend Cooldown rejection.
            Ok(serde_json::json!({
                "variant": our_variant.as_str(),
                "targeted": nearest_hash.is_some(),
                "cooldown_ms": cooldown_ms,
            }))
        }
        EmoteFireResult::Cooldown { remaining_ms } => Err(format!(
            "Emote on cooldown ({remaining_ms}ms remaining)"
        )),
        EmoteFireResult::NoTarget => Err("No target in range".to_string()),
        EmoteFireResult::TargetBlocked => Err("Player is blocked".to_string()),
    }
}

/// Shared core: block-check + fire + publish. Used by both the generic
/// `emote` Tauri command and `emote_hi` (which must keep its Hi-specific
/// daily-gate semantics outside this helper). Assumes any command-specific
/// preflight (range check, daily gate, target validation) has already run.
fn fire_and_publish_emote(
    app: &AppHandle,
    kind: emote::EmoteKind,
    target_bytes: Option<[u8; 16]>,
    our_address: [u8; 16],
) -> Result<EmoteFireResult, String> {
    // Blocked-target check + fire — under a single game-state lock to avoid
    // a TOCTOU gap with a concurrent buddy_block command.
    let result = {
        let state_wrapper = app.state::<GameStateWrapper>();
        let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

        if let Some(t) = target_bytes {
            if state.social.buddies.is_blocked(&t) {
                return Ok(EmoteFireResult::TargetBlocked);
            }
        }

        let social = &mut state.social;
        fire_emote(
            &mut social.emotes,
            &mut social.mood,
            our_address,
            &kind,
            target_bytes,
            std::time::Instant::now(),
        )
    };

    if matches!(result, EmoteFireResult::Success { .. }) {
        let msg = emote::EmoteMessage { kind, target: target_bytes };
        let net = app.state::<NetworkWrapper>();
        let mut net_state = net.0.lock().map_err(|e| e.to_string())?;
        let actions = net_state.publish_emote(msg, &mut rand::rngs::OsRng);
        drop(net_state);
        execute_network_actions(app, actions);
    }

    Ok(result)
}

#[tauri::command]
fn emote(
    kind: emote::EmoteKind,
    target: Option<String>,
    app: AppHandle,
) -> Result<EmoteFireResult, String> {
    // Hi has its own daily gate + viral variant selection semantics that live
    // in `emote_hi`. Routing Hi through this generic path would skip those
    // checks and let a direct IPC call send unlimited custom-variant Hi's.
    if matches!(kind, emote::EmoteKind::Hi(_)) {
        return Err("Hi emotes must use the emote_hi endpoint".to_string());
    }

    let mut target_bytes: Option<[u8; 16]> = match target {
        Some(hex_str) => {
            let bytes = hex::decode(&hex_str).map_err(|_| "Invalid target hash".to_string())?;
            if bytes.len() != 16 {
                return Err("Target hash must be 16 bytes".to_string());
            }
            let mut addr = [0u8; 16];
            addr.copy_from_slice(&bytes);
            Some(addr)
        }
        None => None,
    };

    // Hardening: Dance is the only pure broadcast-only kind — its receive
    // logic awards witness mood only when `target.is_none()`. Applaud is
    // dual-nature (targeted OR witness), so we preserve any target supplied
    // by the UI; the receive path handles both cases correctly.
    if matches!(emote::EmoteKindTag::from(&kind), emote::EmoteKindTag::Dance) {
        target_bytes = None;
    }

    let our_address = {
        let net = app.state::<NetworkWrapper>();
        let net_state = net.0.lock().map_err(|e| e.to_string())?;
        net_state.our_address_hash()
    };

    // Range check for emotes that require physical proximity (hug / high-five).
    // Must run before the fire lock so we can safely read net_state without
    // holding both locks simultaneously.
    let tag = emote::EmoteKindTag::from(&kind);
    let needs_range_check = matches!(tag, emote::EmoteKindTag::Hug | emote::EmoteKindTag::HighFive);

    if needs_range_check {
        if let Some(target) = target_bytes {
            let (self_x, self_y) = {
                let state_wrapper = app.state::<GameStateWrapper>();
                let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
                (state.player.x, state.player.y)
            };

            let target_pos = {
                let net = app.state::<NetworkWrapper>();
                let net_state = net.0.lock().map_err(|e| e.to_string())?;
                net_state
                    .remote_frames()
                    .iter()
                    .find(|rf| {
                        hex::decode(&rf.address_hash)
                            .ok()
                            .and_then(|b| b.try_into().ok())
                            .map(|a: [u8; 16]| a == target)
                            .unwrap_or(false)
                    })
                    .map(|rf| (rf.x, rf.y))
            };

            match target_pos {
                None => return Ok(EmoteFireResult::NoTarget),
                Some((tx, ty)) => {
                    if !is_target_in_range(self_x, self_y, tx, ty, 400.0) {
                        return Ok(EmoteFireResult::NoTarget);
                    }
                }
            }
        } else {
            return Ok(EmoteFireResult::NoTarget);
        }
    }

    fire_and_publish_emote(&app, kind, target_bytes, our_address)
}

#[tauri::command]
fn set_emote_privacy(
    kind: emote::EmoteKind,
    accept: bool,
    app: AppHandle,
) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    state.social.emotes.set_privacy(emote::EmoteKindTag::from(&kind), accept);
    Ok(())
}

#[tauri::command]
fn get_emote_privacy(app: AppHandle) -> Result<serde_json::Value, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "hug": state.social.emotes.accept_hug,
        "high_five": state.social.emotes.accept_high_five,
    }))
}

// ── Social: Buddies ──────────────────────────────────────────────────────────

#[tauri::command]
fn buddy_request(peer_hash: String, app: AppHandle) -> Result<(), String> {
    let peer_bytes: [u8; 16] = hex::decode(&peer_hash)
        .map_err(|_| "Invalid peer hash".to_string())?
        .try_into()
        .map_err(|_| "Peer hash must be 16 bytes".to_string())?;

    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    if state.social.buddies.is_buddy(&peer_bytes) {
        return Err("Already a buddy".to_string());
    }
    if state.social.buddies.is_blocked(&peer_bytes) {
        return Err("Player is blocked".to_string());
    }
    if state.social.buddies.has_outgoing_request(&peer_bytes) {
        return Err("Buddy request already pending".to_string());
    }
    let now = now_secs(&app);
    state.social.buddies.record_outgoing_request(peer_bytes, now);
    drop(state);

    let net = app.state::<NetworkWrapper>();
    let mut net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_hash = net_state.our_address_hash();
    let actions = net_state.publish_social(
        social::SocialMessage::BuddyRequest { from: our_hash, to: peer_bytes },
        &mut rand::rngs::OsRng,
    );
    drop(net_state);
    execute_network_actions(&app, actions);
    Ok(())
}

#[tauri::command]
fn buddy_accept(peer_hash: String, app: AppHandle) -> Result<(), String> {
    let peer_bytes: [u8; 16] = hex::decode(&peer_hash)
        .map_err(|_| "Invalid peer hash".to_string())?
        .try_into()
        .map_err(|_| "Peer hash must be 16 bytes".to_string())?;

    let now = now_secs(&app);
    let today = today_date_string();

    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    let from_name = state
        .social
        .buddies
        .get_pending_request(&peer_bytes, now)
        .map(|r| r.from_name.clone())
        .ok_or_else(|| "No pending buddy request from this player".to_string())?;

    state.social.buddies.add_buddy(social::buddy::BuddyEntry {
        address_hash: peer_bytes,
        display_name: from_name,
        added_date: today,
        co_presence_total: 0.0,
        last_seen_date: None,
    });
    drop(state);

    let net = app.state::<NetworkWrapper>();
    let mut net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_hash = net_state.our_address_hash();
    let actions = net_state.publish_social(
        social::SocialMessage::BuddyAccept { from: our_hash, to: peer_bytes },
        &mut rand::rngs::OsRng,
    );
    drop(net_state);
    execute_network_actions(&app, actions);
    Ok(())
}

#[tauri::command]
fn buddy_decline(peer_hash: String, app: AppHandle) -> Result<(), String> {
    let peer_bytes: [u8; 16] = hex::decode(&peer_hash)
        .map_err(|_| "Invalid peer hash".to_string())?
        .try_into()
        .map_err(|_| "Peer hash must be 16 bytes".to_string())?;

    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    state.social.buddies.remove_pending_request(&peer_bytes);
    drop(state);

    let net = app.state::<NetworkWrapper>();
    let mut net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_hash = net_state.our_address_hash();
    let actions = net_state.publish_social(
        social::SocialMessage::BuddyDecline { from: our_hash, to: peer_bytes },
        &mut rand::rngs::OsRng,
    );
    drop(net_state);
    execute_network_actions(&app, actions);
    Ok(())
}

#[tauri::command]
fn buddy_remove(peer_hash: String, app: AppHandle) -> Result<(), String> {
    let peer_bytes: [u8; 16] = hex::decode(&peer_hash)
        .map_err(|_| "Invalid peer hash".to_string())?
        .try_into()
        .map_err(|_| "Peer hash must be 16 bytes".to_string())?;

    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    if !state.social.buddies.remove_buddy(&peer_bytes) {
        return Err("Not a buddy".to_string());
    }
    drop(state);

    let net = app.state::<NetworkWrapper>();
    let mut net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_hash = net_state.our_address_hash();
    let actions = net_state.publish_social(
        social::SocialMessage::BuddyRemove { from: our_hash, to: peer_bytes },
        &mut rand::rngs::OsRng,
    );
    drop(net_state);
    execute_network_actions(&app, actions);
    Ok(())
}

#[tauri::command]
fn block_player(peer_hash: String, app: AppHandle) -> Result<(), String> {
    let peer_bytes: [u8; 16] = hex::decode(&peer_hash)
        .map_err(|_| "Invalid peer hash".to_string())?
        .try_into()
        .map_err(|_| "Peer hash must be 16 bytes".to_string())?;

    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    state.social.buddies.block_player(&peer_bytes);
    Ok(())
}

#[tauri::command]
fn unblock_player(peer_hash: String, app: AppHandle) -> Result<(), String> {
    let peer_bytes: [u8; 16] = hex::decode(&peer_hash)
        .map_err(|_| "Invalid peer hash".to_string())?
        .try_into()
        .map_err(|_| "Peer hash must be 16 bytes".to_string())?;

    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    state.social.buddies.unblock_player(&peer_bytes);
    Ok(())
}

#[tauri::command]
fn get_buddy_list(app: AppHandle) -> Result<serde_json::Value, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    let buddies: Vec<serde_json::Value> = state
        .social
        .buddies
        .buddies
        .iter()
        .map(|b| {
            serde_json::json!({
                "addressHash": hex::encode(b.address_hash),
                "displayName": b.display_name,
                "addedDate": b.added_date,
                "coPresenceTotal": b.co_presence_total,
                "lastSeenDate": b.last_seen_date,
            })
        })
        .collect();

    Ok(serde_json::json!({ "buddies": buddies }))
}

#[tauri::command]
fn get_blocked_list(app: AppHandle) -> Result<serde_json::Value, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    let blocked: Vec<String> = state
        .social
        .buddies
        .blocked
        .iter()
        .map(|a| hex::encode(a))
        .collect();

    Ok(serde_json::json!({ "blocked": blocked }))
}

// ── Social: Parties ──────────────────────────────────────────────────────────

#[tauri::command]
fn party_invite(peer_hash: String, app: AppHandle) -> Result<(), String> {
    let peer_bytes: [u8; 16] = hex::decode(&peer_hash)
        .map_err(|_| "Invalid peer hash".to_string())?
        .try_into()
        .map_err(|_| "Peer hash must be 16 bytes".to_string())?;

    let now = now_secs(&app);

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let pi = app.state::<PlayerIdentityWrapper>();
    let our_name = pi.display_name.lock().map_err(|e| e.to_string())?.clone();

    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    if state.social.buddies.is_blocked(&peer_bytes) {
        return Err("Player is blocked".to_string());
    }

    // Create party if not already in one.
    if !state.social.party.in_party() {
        state.social.party.create_party(our_address, our_name, now);
    }

    let party = state
        .social
        .party
        .party
        .as_ref()
        .ok_or("Not in a party")?;

    if !party.is_leader(&our_address) {
        return Err("Only the party leader can invite".to_string());
    }

    if party.members.len() >= social::party::MAX_PARTY_SIZE {
        return Err("Party is full".to_string());
    }
    if party.is_member(&peer_bytes) {
        return Err("Player is already in the party".to_string());
    }

    let member_hashes = party.member_hashes();
    state.social.party.record_outgoing_invite(peer_bytes, now);
    drop(state);

    // Broadcast the invite
    let actions = {
        let net = app.state::<NetworkWrapper>();
        let mut net_state = net.0.lock().map_err(|e| e.to_string())?;
        net_state.publish_social(
            social::SocialMessage::PartyInvite {
                leader: our_address,
                to: peer_bytes,
                members: member_hashes,
            },
            &mut rand::rngs::OsRng,
        )
    };
    execute_network_actions(&app, actions);
    Ok(())
}

#[tauri::command]
fn party_accept(app: AppHandle) -> Result<(), String> {
    let now = now_secs(&app);

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let pi = app.state::<PlayerIdentityWrapper>();
    let our_name = pi.display_name.lock().map_err(|e| e.to_string())?.clone();

    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    let invite_leader = state
        .social
        .party
        .begin_join(our_name, now)
        .map_err(|e| e.to_string())?;
    drop(state);

    let actions = {
        let net = app.state::<NetworkWrapper>();
        let mut net_state = net.0.lock().map_err(|e| e.to_string())?;
        net_state.publish_social(
            social::SocialMessage::PartyAccept { from: our_address, to: invite_leader },
            &mut rand::rngs::OsRng,
        )
    };
    execute_network_actions(&app, actions);
    Ok(())
}

#[tauri::command]
fn party_decline(app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    let invite_leader = state
        .social
        .party
        .pending_invite
        .as_ref()
        .map(|i| i.leader)
        .ok_or_else(|| "No pending invite".to_string())?;
    state.social.party.decline_invite();
    drop(state);

    let net = app.state::<NetworkWrapper>();
    let mut net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    let actions = net_state.publish_social(
        social::SocialMessage::PartyDecline { from: our_address, to: invite_leader },
        &mut rand::rngs::OsRng,
    );
    drop(net_state);
    execute_network_actions(&app, actions);
    Ok(())
}

#[tauri::command]
fn party_leave(app: AppHandle) -> Result<(), String> {
    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    state
        .social
        .party
        .leave_party(&our_address)
        .map_err(|e| e.to_string())?;
    state.social.party.clear_outgoing_invites();
    drop(state);

    let actions = {
        let net = app.state::<NetworkWrapper>();
        let mut net_state = net.0.lock().map_err(|e| e.to_string())?;
        net_state.publish_social(
            social::SocialMessage::PartyLeave { from: our_address },
            &mut rand::rngs::OsRng,
        )
    };
    execute_network_actions(&app, actions);
    Ok(())
}

#[tauri::command]
fn party_kick(peer_hash: String, app: AppHandle) -> Result<(), String> {
    let peer_bytes: [u8; 16] = hex::decode(&peer_hash)
        .map_err(|_| "Invalid peer hash".to_string())?
        .try_into()
        .map_err(|_| "Peer hash must be 16 bytes".to_string())?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    let party = state
        .social
        .party
        .party
        .as_mut()
        .ok_or("Not in a party")?;

    party
        .kick_member(&our_address, &peer_bytes)
        .map_err(|e| e.to_string())?;
    drop(state);

    let actions = {
        let net = app.state::<NetworkWrapper>();
        let mut net_state = net.0.lock().map_err(|e| e.to_string())?;
        net_state.publish_social(
            social::SocialMessage::PartyKick { from: our_address, target: peer_bytes },
            &mut rand::rngs::OsRng,
        )
    };
    execute_network_actions(&app, actions);
    Ok(())
}

#[tauri::command]
fn get_party_state(app: AppHandle) -> Result<serde_json::Value, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    match &state.social.party.party {
        None => Ok(serde_json::json!({
            "inParty": false,
            "leader": null,
            "members": [],
        })),
        Some(party) => {
            let members: Vec<serde_json::Value> = party
                .members
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "addressHash": hex::encode(m.address_hash),
                        "displayName": m.display_name,
                        "isLeader": m.address_hash == party.leader,
                    })
                })
                .collect();
            Ok(serde_json::json!({
                "inParty": true,
                "leader": hex::encode(party.leader),
                "members": members,
            }))
        }
    }
}

// ── Group helpers ────────────────────────────────────────────────────────

fn parse_group_id(hex_str: &str) -> Result<[u8; 16], String> {
    let bytes = hex::decode(hex_str).map_err(|_| "Invalid group ID hex".to_string())?;
    let arr: [u8; 16] = bytes
        .try_into()
        .map_err(|_| "Group ID must be 16 bytes".to_string())?;
    Ok(arr)
}

// Dissolve is terminal: once a group is dissolved, no new member/role/info ops
// should append to its log, and `get_my_groups` already filters dissolved
// groups out of the UI. Mutating handlers must reject dissolved state here so
// callers can't quietly build invisible inconsistent state (e.g. joining an
// open group that no longer shows up in their list).
fn validate_group_active(state: &harmony_groups::GroupState) -> Result<(), String> {
    if state.dissolved {
        Err("Group is dissolved".to_string())
    } else {
        Ok(())
    }
}

fn publish_group_op(app: &AppHandle, group_id: [u8; 16], op: harmony_groups::GroupOp) {
    let net = app.state::<NetworkWrapper>();
    let actions = {
        let mut net_state = match net.0.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        net_state.publish_group_op(group_id, op, &mut rand::rngs::OsRng)
    };
    execute_network_actions(app, actions);
}

fn serialize_group_state(state: &harmony_groups::GroupState) -> serde_json::Value {
    let founder_addr = state
        .members
        .iter()
        .find(|(_, e)| e.role == harmony_groups::Role::Founder)
        .map(|(a, _)| *a);
    let members: Vec<serde_json::Value> = state
        .members
        .iter()
        .map(|(addr, entry)| {
            serde_json::json!({
                "addressHash": hex::encode(addr),
                "role": match entry.role {
                    harmony_groups::Role::Founder => "founder",
                    harmony_groups::Role::Officer => "officer",
                    harmony_groups::Role::Member => "member",
                },
                "joinedAt": entry.joined_at,
                "isFounder": Some(*addr) == founder_addr,
            })
        })
        .collect();
    serde_json::json!({
        "groupId": hex::encode(state.group_id),
        "name": state.name,
        "mode": if state.mode == harmony_groups::GroupMode::Open { "open" } else { "invite_only" },
        "founderHash": founder_addr.map(|a| hex::encode(a)),
        "members": members,
        "memberCount": state.members.len(),
        "dissolved": state.dissolved,
    })
}

// ── Group IPC commands ──────────────────────────────────────────────────

#[tauri::command]
fn group_create(name: String, mode: String, app: AppHandle) -> Result<String, String> {
    let name = normalize_group_name(&name)?;
    let group_mode = match mode.as_str() {
        "open" => harmony_groups::GroupMode::Open,
        "invite_only" => harmony_groups::GroupMode::InviteOnly,
        _ => return Err("Invalid mode: must be 'open' or 'invite_only'".to_string()),
    };

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let mut group_id = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut group_id);

    let (op, _) = harmony_groups::GroupOp::new_unsigned(
        vec![],
        our_address,
        group_op_timestamp(),
        harmony_groups::GroupAction::Create {
            group_id,
            name: name.clone(),
            mode: group_mode,
        },
    );

    let gm = app.state::<GroupManagerWrapper>();
    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    groups.merge_op(group_id, op.clone())?;
    drop(groups);

    publish_group_op(&app, group_id, op);

    let hex_id = hex::encode(group_id);
    let _ = app.emit(
        "group_created",
        serde_json::json!({
            "groupId": hex_id,
            "name": name,
        }),
    );
    Ok(hex_id)
}

#[tauri::command]
fn group_invite(group_id_hex: String, peer_hash: String, app: AppHandle) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;
    let peer_bytes: [u8; 16] = hex::decode(&peer_hash)
        .map_err(|_| "Invalid peer hash".to_string())?
        .try_into()
        .map_err(|_| "Peer hash must be 16 bytes".to_string())?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let op = {
        let mut groups = gm.0.lock().map_err(|e| e.to_string())?;

        let state = groups
            .get_state(group_id)
            .ok_or_else(|| "Group not found".to_string())?;
        validate_group_active(state)?;

        // Validate author is Officer or Founder.
        match state.role_of(&our_address) {
            Some(harmony_groups::Role::Founder) | Some(harmony_groups::Role::Officer) => {}
            _ => return Err("Only Officers and Founders can invite".to_string()),
        }

        // Target must not already be a member.
        if state.is_member(&peer_bytes) {
            return Err("Player is already a member".to_string());
        }

        let parents = groups.head_ops(group_id);
        let (op, _) = harmony_groups::GroupOp::new_unsigned(
            parents,
            our_address,
            group_op_timestamp(),
            harmony_groups::GroupAction::Invite { invitee: peer_bytes },
        );
        groups.merge_op(group_id, op.clone())?;
        op
    };

    publish_group_op(&app, group_id, op);

    let _ = app.emit(
        "group_invite_sent",
        serde_json::json!({
            "groupId": group_id_hex,
            "inviteeHash": peer_hash,
        }),
    );
    Ok(())
}

#[tauri::command]
fn group_accept(group_id_hex: String, app: AppHandle) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let (op, group_name) = {
        let mut groups = gm.0.lock().map_err(|e| e.to_string())?;

        // If we already have state for this group and it's dissolved, accepting
        // would re-add us to a tombstoned log. Reject before touching the log.
        // (First-time accepts won't have state yet — that path is fine.)
        if let Some(state) = groups.get_state(group_id) {
            validate_group_active(state)?;
        }

        // Prefer the ephemeral pending_invites map (fast path with cached
        // display name), but fall back to the persisted op log after restart.
        let (invite_op, group_name) = match groups.pending_invites.get(&group_id).cloned() {
            Some(p) => (p.invite_op, p.group_name),
            None => {
                let invite_op = groups
                    .find_outstanding_invite(group_id, our_address)
                    .ok_or_else(|| "No pending invite for this group".to_string())?;
                let gname = groups
                    .get_state(group_id)
                    .map(|s| s.name.clone())
                    .unwrap_or_default();
                (invite_op, gname)
            }
        };

        groups.merge_op(group_id, invite_op.clone())?;
        let parents = groups.head_ops(group_id);
        let (op, _) = harmony_groups::GroupOp::new_unsigned(
            parents,
            our_address,
            group_op_timestamp(),
            harmony_groups::GroupAction::Accept {
                invite_op: invite_op.id,
            },
        );
        groups.merge_op(group_id, op.clone())?;
        groups.pending_invites.remove(&group_id);
        (op, group_name)
    };

    publish_group_op(&app, group_id, op);

    let _ = app.emit(
        "group_joined",
        serde_json::json!({
            "groupId": group_id_hex,
            "groupName": group_name,
        }),
    );
    Ok(())
}

#[tauri::command]
fn group_decline(group_id_hex: String, app: AppHandle) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;

    let net = app.state::<NetworkWrapper>();
    let our_address = {
        let net_state = net.0.lock().map_err(|e| e.to_string())?;
        net_state.our_address_hash()
    };

    let gm = app.state::<GroupManagerWrapper>();
    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;

    if !groups.decline_invite(group_id, our_address)? {
        return Err("No pending invite for this group".to_string());
    }

    Ok(())
}

#[tauri::command]
fn group_join(group_id_hex: String, app: AppHandle) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let op = {
        let mut groups = gm.0.lock().map_err(|e| e.to_string())?;

        let state = groups
            .get_state(group_id)
            .ok_or_else(|| "Group not found".to_string())?;
        validate_group_active(state)?;

        if state.mode != harmony_groups::GroupMode::Open {
            return Err("Group is invite-only".to_string());
        }
        if state.is_member(&our_address) {
            return Err("Already a member".to_string());
        }

        let parents = groups.head_ops(group_id);
        let (op, _) = harmony_groups::GroupOp::new_unsigned(
            parents,
            our_address,
            group_op_timestamp(),
            harmony_groups::GroupAction::Join,
        );
        groups.merge_op(group_id, op.clone())?;
        op
    };

    publish_group_op(&app, group_id, op);

    let _ = app.emit(
        "group_joined",
        serde_json::json!({
            "groupId": group_id_hex,
        }),
    );
    Ok(())
}

#[tauri::command]
fn group_leave(group_id_hex: String, app: AppHandle) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let op = {
        let mut groups = gm.0.lock().map_err(|e| e.to_string())?;

        let state = groups
            .get_state(group_id)
            .ok_or_else(|| "Group not found".to_string())?;
        validate_group_active(state)?;

        if !state.is_member(&our_address) {
            return Err("Not a member of this group".to_string());
        }

        let parents = groups.head_ops(group_id);
        let (op, _) = harmony_groups::GroupOp::new_unsigned(
            parents,
            our_address,
            group_op_timestamp(),
            harmony_groups::GroupAction::Leave,
        );
        groups.merge_op(group_id, op.clone())?;
        op
    };

    publish_group_op(&app, group_id, op);

    let _ = app.emit(
        "group_left",
        serde_json::json!({
            "groupId": group_id_hex,
        }),
    );
    Ok(())
}

#[tauri::command]
fn group_kick(
    group_id_hex: String,
    peer_hash: String,
    app: AppHandle,
) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;
    let peer_bytes: [u8; 16] = hex::decode(&peer_hash)
        .map_err(|_| "Invalid peer hash".to_string())?
        .try_into()
        .map_err(|_| "Peer hash must be 16 bytes".to_string())?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let op = {
        let mut groups = gm.0.lock().map_err(|e| e.to_string())?;

        let state = groups
            .get_state(group_id)
            .ok_or_else(|| "Group not found".to_string())?;
        validate_group_active(state)?;

        // Explicit self-kick guard: `outranks` is strict (`<`, not `<=`) today,
        // so a Founder can't kick themselves via the rank check. But that
        // surfaces as a misleading "Insufficient rank" error, and if the
        // harmony_groups `outranks` semantics ever relax, a Founder could
        // unknowingly remove themselves. Check the target explicitly.
        if peer_bytes == our_address {
            return Err("Cannot kick yourself — use Leave or Dissolve instead".to_string());
        }

        let our_role = state
            .role_of(&our_address)
            .ok_or_else(|| "Not a member of this group".to_string())?;
        let target_role = state
            .role_of(&peer_bytes)
            .ok_or_else(|| "Target is not a member".to_string())?;

        if !our_role.outranks(target_role) {
            return Err("Insufficient rank to kick this member".to_string());
        }

        let parents = groups.head_ops(group_id);
        let (op, _) = harmony_groups::GroupOp::new_unsigned(
            parents,
            our_address,
            group_op_timestamp(),
            harmony_groups::GroupAction::Kick { target: peer_bytes },
        );
        groups.merge_op(group_id, op.clone())?;
        op
    };

    publish_group_op(&app, group_id, op);

    let _ = app.emit(
        "group_member_kicked",
        serde_json::json!({
            "groupId": group_id_hex,
            "targetHash": peer_hash,
        }),
    );
    Ok(())
}

#[tauri::command]
fn group_promote(
    group_id_hex: String,
    peer_hash: String,
    app: AppHandle,
) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;
    let peer_bytes: [u8; 16] = hex::decode(&peer_hash)
        .map_err(|_| "Invalid peer hash".to_string())?
        .try_into()
        .map_err(|_| "Peer hash must be 16 bytes".to_string())?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let op = {
        let mut groups = gm.0.lock().map_err(|e| e.to_string())?;

        let state = groups
            .get_state(group_id)
            .ok_or_else(|| "Group not found".to_string())?;
        validate_group_active(state)?;

        if state.role_of(&our_address) != Some(harmony_groups::Role::Founder) {
            return Err("Only the Founder can promote members".to_string());
        }
        if !state.is_member(&peer_bytes) {
            return Err("Target is not a member".to_string());
        }

        let parents = groups.head_ops(group_id);
        let (op, _) = harmony_groups::GroupOp::new_unsigned(
            parents,
            our_address,
            group_op_timestamp(),
            harmony_groups::GroupAction::Promote { target: peer_bytes },
        );
        groups.merge_op(group_id, op.clone())?;
        op
    };

    publish_group_op(&app, group_id, op);

    let _ = app.emit(
        "group_member_promoted",
        serde_json::json!({
            "groupId": group_id_hex,
            "targetHash": peer_hash,
        }),
    );
    Ok(())
}

#[tauri::command]
fn group_demote(
    group_id_hex: String,
    peer_hash: String,
    app: AppHandle,
) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;
    let peer_bytes: [u8; 16] = hex::decode(&peer_hash)
        .map_err(|_| "Invalid peer hash".to_string())?
        .try_into()
        .map_err(|_| "Peer hash must be 16 bytes".to_string())?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let op = {
        let mut groups = gm.0.lock().map_err(|e| e.to_string())?;

        let state = groups
            .get_state(group_id)
            .ok_or_else(|| "Group not found".to_string())?;
        validate_group_active(state)?;

        if state.role_of(&our_address) != Some(harmony_groups::Role::Founder) {
            return Err("Only the Founder can demote members".to_string());
        }
        if state.role_of(&peer_bytes) != Some(harmony_groups::Role::Officer) {
            return Err("Can only demote Officers".to_string());
        }

        let parents = groups.head_ops(group_id);
        let (op, _) = harmony_groups::GroupOp::new_unsigned(
            parents,
            our_address,
            group_op_timestamp(),
            harmony_groups::GroupAction::Demote { target: peer_bytes },
        );
        groups.merge_op(group_id, op.clone())?;
        op
    };

    publish_group_op(&app, group_id, op);

    let _ = app.emit(
        "group_member_demoted",
        serde_json::json!({
            "groupId": group_id_hex,
            "targetHash": peer_hash,
        }),
    );
    Ok(())
}

#[tauri::command]
fn group_dissolve(group_id_hex: String, app: AppHandle) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let op = {
        let mut groups = gm.0.lock().map_err(|e| e.to_string())?;

        let state = groups
            .get_state(group_id)
            .ok_or_else(|| "Group not found".to_string())?;
        validate_group_active(state)?;

        if state.role_of(&our_address) != Some(harmony_groups::Role::Founder) {
            return Err("Only the Founder can dissolve the group".to_string());
        }

        let parents = groups.head_ops(group_id);
        let (op, _) = harmony_groups::GroupOp::new_unsigned(
            parents,
            our_address,
            group_op_timestamp(),
            harmony_groups::GroupAction::Dissolve,
        );
        groups.merge_op(group_id, op.clone())?;
        op
    };

    publish_group_op(&app, group_id, op);

    let _ = app.emit(
        "group_dissolved",
        serde_json::json!({
            "groupId": group_id_hex,
        }),
    );
    Ok(())
}

#[tauri::command]
fn group_update_info(
    group_id_hex: String,
    name: Option<String>,
    mode: Option<String>,
    app: AppHandle,
) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;

    let name = match name {
        Some(raw) => Some(normalize_group_name(&raw)?),
        None => None,
    };

    let group_mode = match mode.as_deref() {
        Some("open") => Some(harmony_groups::GroupMode::Open),
        Some("invite_only") => Some(harmony_groups::GroupMode::InviteOnly),
        Some(_) => return Err("Invalid mode: must be 'open' or 'invite_only'".to_string()),
        None => None,
    };

    // Reject fully empty updates before we persist a no-op op to the log
    // and broadcast it to every peer.
    if name.is_none() && group_mode.is_none() {
        return Err("At least one of name or mode must be provided".to_string());
    }

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let op = {
        let mut groups = gm.0.lock().map_err(|e| e.to_string())?;

        let state = groups
            .get_state(group_id)
            .ok_or_else(|| "Group not found".to_string())?;
        validate_group_active(state)?;

        if state.role_of(&our_address) != Some(harmony_groups::Role::Founder) {
            return Err("Only the Founder can update group info".to_string());
        }

        let parents = groups.head_ops(group_id);
        let (op, _) = harmony_groups::GroupOp::new_unsigned(
            parents,
            our_address,
            group_op_timestamp(),
            harmony_groups::GroupAction::UpdateInfo {
                name: name.clone(),
                mode: group_mode,
            },
        );
        groups.merge_op(group_id, op.clone())?;
        op
    };

    publish_group_op(&app, group_id, op);

    let _ = app.emit(
        "group_info_updated",
        serde_json::json!({
            "groupId": group_id_hex,
        }),
    );
    Ok(())
}

#[tauri::command]
fn get_group_state(group_id_hex: String, app: AppHandle) -> Result<serde_json::Value, String> {
    let group_id = parse_group_id(&group_id_hex)?;

    let gm = app.state::<GroupManagerWrapper>();
    let groups = gm.0.lock().map_err(|e| e.to_string())?;

    let state = groups
        .get_state(group_id)
        .ok_or_else(|| "Group not found".to_string())?;

    Ok(serialize_group_state(state))
}

#[tauri::command]
fn get_my_groups(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let groups = gm.0.lock().map_err(|e| e.to_string())?;

    let my = groups.my_groups(our_address);
    let result: Vec<serde_json::Value> = my.iter().map(|s| serialize_group_state(s)).collect();
    Ok(result)
}

/// Return all currently-pending group invites for the local user.
///
/// The frontend calls this on mount to recover invites that were rebuilt
/// from the persisted op log during `setup()` — those can't be delivered
/// via `group_invite_received` events because no listener is registered
/// at setup time.
#[tauri::command]
fn get_pending_invites(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let gm = app.state::<GroupManagerWrapper>();
    let groups = gm.0.lock().map_err(|e| e.to_string())?;

    let result: Vec<serde_json::Value> = groups
        .pending_invites
        .iter()
        .map(|(gid, p)| {
            serde_json::json!({
                "groupId": hex::encode(gid),
                "inviterHash": hex::encode(p.inviter),
                "opId": hex::encode(p.invite_op.id),
                "groupName": p.group_name,
                "inviterName": p.inviter_name,
            })
        })
        .collect();
    Ok(result)
}

/// Returns today's date as a "YYYY-MM-DD" string using the system clock.
fn today_date_string() -> String {
    crate::date_util::today_date_string()
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
            NetworkAction::EmoteReceived { sender, emote } => {
                // Look up our address, sender name, and positions under Net lock.
                let (our_address, sender_name, sender_pos, self_pos) = {
                    let net = app.state::<NetworkWrapper>();
                    let net_state = net.0.lock().unwrap_or_else(|e| e.into_inner());
                    let our_addr = net_state.our_address_hash();
                    let name = net_state.peer_display_name(&sender).unwrap_or_default();
                    let sender_pos = net_state
                        .remote_frames()
                        .iter()
                        .find(|rf| {
                            hex::decode(&rf.address_hash)
                                .ok()
                                .and_then(|b| b.try_into().ok())
                                .map(|a: [u8; 16]| a == sender)
                                .unwrap_or(false)
                        })
                        .map(|rf| (rf.x, rf.y));
                    drop(net_state);

                    let state_wrapper = app.state::<GameStateWrapper>();
                    let state = state_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());
                    let self_pos = (state.player.x, state.player.y);
                    (our_addr, name, sender_pos, self_pos)
                };

                // Defense-in-depth: coerce broadcast-only kinds back to
                // broadcast regardless of what the sender wired. See
                // effective_receive_target for the rationale.
                let effective_target = effective_receive_target(&emote);
                let we_are_target = effective_target.map(|t| t == our_address).unwrap_or(false);

                // Drop malformed hug/high-five packets with no target. Sender-
                // side rejects these, but a modified peer could still wire
                // target=None; a missing-target hug is nonsense regardless.
                let tag_pre = emote::EmoteKindTag::from(&emote.kind);
                if matches!(tag_pre, emote::EmoteKindTag::Hug | emote::EmoteKindTag::HighFive)
                    && effective_target.is_none()
                {
                    continue;
                }

                // Drop targeted emotes aimed at someone else — EXCEPT Applaud,
                // which is dual-nature. For Applaud a non-target is still a
                // potential witness; we fall through to the witness-radius
                // check in the mood routing below.
                let is_applaud = matches!(emote.kind, emote::EmoteKind::Applaud);
                if effective_target.is_some() && !we_are_target && !is_applaud {
                    continue;
                }

                let state_wrapper = app.state::<GameStateWrapper>();
                let mut state = state_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());

                // Block list drop
                if state.social.buddies.is_blocked(&sender) {
                    continue;
                }

                // Privacy toggle drop (hug / high_five only; others unconditional)
                let tag = emote::EmoteKindTag::from(&emote.kind);
                if !state.social.emotes.privacy_accepts(tag) {
                    continue;
                }

                // Range validation for targeted hug/high-five — mirrors the
                // 400px sender-side check. A modified client could otherwise
                // deliver a hug from arbitrary range and still affect us.
                if matches!(tag, emote::EmoteKindTag::Hug | emote::EmoteKindTag::HighFive)
                    && we_are_target
                {
                    let in_range = sender_pos
                        .map(|(sx, sy)| is_target_in_range(self_pos.0, self_pos.1, sx, sy, 400.0))
                        .unwrap_or(false);
                    if !in_range {
                        continue;
                    }
                }

                // Receiver-side fire-cooldown mirror — enforces ONLY the
                // per-pair cooldown (hug 60s / high-five 30s). Global
                // anti-mash cooldown is a local outbound throttle and must
                // not be consumed by inbound traffic from other senders.
                let now = std::time::Instant::now();
                if state
                    .social
                    .emotes
                    .cooldowns
                    .check_receive(now, &emote.kind, sender)
                    .is_err()
                {
                    continue;
                }
                state.social.emotes.cooldowns.mark_receive(now, &emote.kind, sender);

                // Route by kind
                let (mood_delta, event_kind) = match &emote.kind {
                    emote::EmoteKind::Hi(variant) => {
                        let delta = state.social.emotes.handle_incoming_hi(
                            sender,
                            *variant,
                            false,
                        );
                        if delta > 0.0 {
                            state.social.mood.apply_mood_change(delta);
                        }
                        (delta, "hi")
                    }
                    other => {
                        // Witness-radius check. Always computed — each kind's
                        // mood routing in apply_receive_mood decides whether
                        // to use it. Dance uses it when broadcast; Applaud
                        // uses it for both targeted-bystander and broadcast
                        // cases; targeted-only kinds (Wave/Hug/HighFive)
                        // ignore it (they already gate on we_are_target).
                        let nearby = sender_pos
                            .map(|(sx, sy)| {
                                let dx = self_pos.0 - sx;
                                let dy = self_pos.1 - sy;
                                (dx * dx + dy * dy).sqrt() <= 300.0
                            })
                            .unwrap_or(false);
                        let social = &mut state.social;
                        let delta = apply_receive_mood(
                            &mut social.emotes,
                            &mut social.mood,
                            sender,
                            other,
                            we_are_target,
                            nearby,
                            now,
                        );
                        let kind_str = match other {
                            emote::EmoteKind::Dance => "dance",
                            emote::EmoteKind::Wave => "wave",
                            emote::EmoteKind::Hug => "hug",
                            emote::EmoteKind::HighFive => "high_five",
                            emote::EmoteKind::Applaud => "applaud",
                            emote::EmoteKind::Hi(_) => unreachable!(),
                        };
                        (delta, kind_str)
                    }
                };

                drop(state);

                let variant_str = match &emote.kind {
                    emote::EmoteKind::Hi(v) => Some(v.as_str()),
                    _ => None,
                };

                let _ = app.emit(
                    "emote_received",
                    serde_json::json!({
                        "senderHash": hex::encode(sender),
                        "senderName": sender_name,
                        "kind": event_kind,
                        "variant": variant_str,
                        "moodDelta": mood_delta,
                    }),
                );
            }
            NetworkAction::SocialReceived { sender, message } => {
                handle_social_message(app, sender, message);
            }
            NetworkAction::GroupOpReceived { sender: _, group_id, op } => {
                // `sender` is whoever relayed this op's packet — which may be
                // the author or any peer that's gossiping it. We don't use it
                // here because the author's identity lives in `op.author`, and
                // that's what downstream consumers need (e.g. the inviter for
                // a PendingGroupInvite). Authenticity of `op.author` depends on
                // signature verification — see the `resolve()` doc in
                // harmony-groups.
                let net = app.state::<NetworkWrapper>();
                let our_addr = match net.0.lock() {
                    Ok(ns) => ns.our_address_hash(),
                    Err(_) => continue,
                };

                let gm = app.state::<GroupManagerWrapper>();
                let mut groups = match gm.0.lock() {
                    Ok(g) => g,
                    Err(_) => continue,
                };

                let (_progressed, applied_ids) =
                    groups.merge_op_with_orphans(group_id, op.clone());

                let group_id_hex = hex::encode(group_id);

                if !applied_ids.is_empty() {
                    let _ = app.emit(
                        "group_state_changed",
                        serde_json::json!({ "groupId": group_id_hex }),
                    );
                }

                // Surface invite prompts for every invite-targeting-us that
                // became applied during this call — covers both the fast
                // path (the incoming op was an invite for us) and the
                // orphan path (an earlier invite for us was buffered and
                // just unblocked by new ancestors).
                let mut pending_to_add: Option<(
                    harmony_groups::OpId,
                    harmony_groups::MemberAddr,
                    harmony_groups::GroupOp,
                    String,
                )> = None;
                // If we ended up a member after this merge (e.g. because an
                // orphaned Accept also applied), don't prompt — the invite
                // has already been consumed. `prune_pending_invite_if_member`
                // also drops any pre-existing pending entry so the frontend
                // isn't left with a ghost invite for a group we're in.
                let already_member =
                    groups.prune_pending_invite_if_member(group_id, our_addr);

                if !already_member {
                    // Keep at most one Invite per merge — concurrent invites
                    // (two Officers inviting us in the same batch) are
                    // semantically equivalent, so store the latest by
                    // timestamp. Prevents one invite event per op while
                    // only the last actually landing in pending_invites.
                    for aid in &applied_ids {
                        let applied_op = match groups.get_ops(group_id).and_then(|ops| {
                            ops.iter().find(|o| o.id == *aid).cloned()
                        }) {
                            Some(o) => o,
                            None => continue,
                        };
                        if let harmony_groups::GroupAction::Invite { invitee } = &applied_op.action {
                            if *invitee == our_addr {
                                let gname = groups
                                    .get_state(group_id)
                                    .map(|s| s.name.clone())
                                    .unwrap_or_default();
                                let replace = match &pending_to_add {
                                    None => true,
                                    Some((_, _, prev, _)) => applied_op.timestamp > prev.timestamp,
                                };
                                if replace {
                                    pending_to_add =
                                        Some((*aid, applied_op.author, applied_op, gname));
                                }
                            }
                        }
                    }
                }

                drop(groups);

                if let Some((oid, inviter, inv_op, gname)) = pending_to_add {
                    // Need the network lock again to look up the display name.
                    let inviter_name = match net.0.lock() {
                        Ok(ns) => ns.peer_display_name(&inviter).unwrap_or_default(),
                        Err(_) => continue,
                    };

                    let mut groups = match gm.0.lock() {
                        Ok(g) => g,
                        Err(_) => continue,
                    };
                    // Re-check membership under the re-acquired lock. If
                    // another thread made us a member (e.g. group_accept ran
                    // while we were looking up the display name),
                    // `prune_pending_invite_if_member` clears any stale entry
                    // and we skip the insert — otherwise we'd write a ghost
                    // invite for a group we're already in.
                    let inserted = if groups
                        .prune_pending_invite_if_member(group_id, our_addr)
                    {
                        false
                    } else {
                        groups.pending_invites.insert(
                            group_id,
                            crate::social::groups::PendingGroupInvite {
                                group_id,
                                inviter,
                                inviter_name: inviter_name.clone(),
                                group_name: gname,
                                invite_op: inv_op,
                                received_at: now_secs(app),
                            },
                        );
                        true
                    };
                    drop(groups);

                    if inserted {
                        let _ = app.emit(
                            "group_invite_received",
                            serde_json::json!({
                                "groupId": group_id_hex,
                                "inviterHash": hex::encode(inviter),
                                "opId": hex::encode(oid),
                            }),
                        );
                    }
                }
            }
        }
    }
}

/// Process an inbound social message from an authenticated peer.
///
/// Three layers of validation before any state mutation:
/// 1. **Sender check:** Every variant's `from`/`leader` must match the
///    authenticated Zenoh session address — prevents identity spoofing.
/// 2. **Recipient check:** Directed variants carry a `to` field; if it
///    doesn't match our address the message isn't for us — drop silently.
/// 3. **Authorisation:** Party-control messages are only accepted from
///    the current party leader (or the member themselves for self-leave).
fn handle_social_message(
    app: &AppHandle,
    authenticated_sender: [u8; 16],
    msg: social::SocialMessage,
) {
    use social::SocialMessage;

    // ── 1. Sender validation ────────────────────────────────────────────
    // Every variant now carries a `from` (or `leader`) field.
    let claimed_sender = match &msg {
        SocialMessage::BuddyRequest { from, .. }
        | SocialMessage::BuddyAccept { from, .. }
        | SocialMessage::BuddyDecline { from, .. }
        | SocialMessage::BuddyRemove { from, .. }
        | SocialMessage::PartyAccept { from, .. }
        | SocialMessage::PartyDecline { from, .. }
        | SocialMessage::PartyLeave { from }
        | SocialMessage::PartyKick { from, .. }
        | SocialMessage::PartyMemberJoined { from, .. }
        | SocialMessage::PartyMemberLeft { from, .. }
        | SocialMessage::PartyDissolved { from }
        | SocialMessage::PartyLeaderChanged { from, .. } => *from,
        SocialMessage::PartyInvite { leader, .. } => *leader,
    };
    if claimed_sender != authenticated_sender {
        return; // spoofed — drop silently
    }

    // ── 2. Recipient filtering ──────────────────────────────────────────
    // Directed messages carry a `to` — ignore if not addressed to us.
    let (sender_name, our_address) = {
        let net = app.state::<NetworkWrapper>();
        let net_state = net.0.lock().unwrap_or_else(|e| e.into_inner());
        (
            net_state
                .peer_display_name(&authenticated_sender)
                .unwrap_or_default(),
            net_state.our_address_hash(),
        )
    };

    let intended_recipient = match &msg {
        SocialMessage::BuddyRequest { to, .. }
        | SocialMessage::BuddyAccept { to, .. }
        | SocialMessage::BuddyDecline { to, .. }
        | SocialMessage::BuddyRemove { to, .. }
        | SocialMessage::PartyInvite { to, .. }
        | SocialMessage::PartyAccept { to, .. }
        | SocialMessage::PartyDecline { to, .. } => Some(*to),
        _ => None, // broadcast party messages — handled by party-state checks
    };
    if let Some(to) = intended_recipient {
        if to != our_address {
            return; // not for us
        }
    }

    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());

    // Drop messages from blocked players
    if state.social.buddies.is_blocked(&authenticated_sender) {
        return;
    }

    let sender_hex = hex::encode(authenticated_sender);

    match msg {
        // ── Buddy operations ────────────────────────────────────────────
        SocialMessage::BuddyRequest { from, .. } => {
            let now = now_secs(app);
            let stored = state
                .social
                .buddies
                .add_pending_request(social::buddy::PendingBuddyRequest {
                    from,
                    from_name: sender_name.clone(),
                    received_at: now,
                });
            if stored {
                let _ = app.emit(
                    "buddy_request_received",
                    serde_json::json!({
                        "fromHash": sender_hex,
                        "fromName": sender_name,
                    }),
                );
            }
        }
        SocialMessage::BuddyAccept { from, .. } => {
            // Only accept if we actually sent them a request.
            if !state.social.buddies.consume_outgoing_request(&from) {
                return; // unsolicited accept — ignore
            }
            let today = today_date_string();
            state.social.buddies.add_buddy(social::buddy::BuddyEntry {
                address_hash: from,
                display_name: sender_name.clone(),
                added_date: today,
                co_presence_total: 0.0,
                last_seen_date: None,
            });
            let _ = app.emit(
                "buddy_accepted",
                serde_json::json!({
                    "fromHash": sender_hex,
                    "fromName": sender_name,
                }),
            );
        }
        SocialMessage::BuddyDecline { .. } => {
            if state.social.buddies.consume_outgoing_request(&authenticated_sender) {
                let _ = app.emit(
                    "buddy_declined",
                    serde_json::json!({ "fromHash": sender_hex }),
                );
            }
        }
        SocialMessage::BuddyRemove { from, .. } => {
            if state.social.buddies.remove_buddy(&from) {
                let _ = app.emit(
                    "buddy_removed",
                    serde_json::json!({ "fromHash": sender_hex }),
                );
            }
        }

        // ── Party invite / accept / decline ─────────────────────────────
        SocialMessage::PartyInvite { leader, members, .. } => {
            let now = now_secs(app);
            state
                .social
                .party
                .set_pending_invite(social::party::PendingPartyInvite {
                    leader,
                    leader_name: sender_name.clone(),
                    members: members.clone(),
                    received_at: now,
                });
            // members already includes the leader (from member_hashes())
            let _ = app.emit(
                "party_invite_received",
                serde_json::json!({
                    "leaderHash": sender_hex,
                    "leaderName": sender_name,
                    "memberCount": members.len(),
                }),
            );
        }
        SocialMessage::PartyAccept { from, .. } => {
            // Check invite exists without consuming — only consume after successful add.
            if !state.social.party.has_outgoing_invite(&from) {
                return; // unsolicited accept — ignore
            }
            let now = now_secs(app);
            if let Some(ref mut party) = state.social.party.party {
                if party.add_member(social::party::PartyMember {
                    address_hash: from,
                    display_name: sender_name.clone(),
                    joined_at: now,
                }).is_ok() {
                    // Only consume the invite after successful add.
                    state.social.party.consume_outgoing_invite(&from);
                    let _ = app.emit(
                        "party_member_joined",
                        serde_json::json!({
                            "memberHash": sender_hex,
                            "memberName": sender_name,
                        }),
                    );
                    // Fan-out: notify other party members about the new join.
                    // Collect data while we hold the game lock, publish after releasing.
                    let member_hash = from;
                    let member_name = sender_name.clone();
                    let our_addr = our_address;
                    drop(state);
                    let net = app.state::<NetworkWrapper>();
                    let mut net_state = net.0.lock().unwrap_or_else(|e| e.into_inner());
                    let actions = net_state.publish_social(
                        social::SocialMessage::PartyMemberJoined {
                            from: our_addr,
                            member: member_hash,
                            display_name: member_name,
                        },
                        &mut rand::rngs::OsRng,
                    );
                    drop(net_state);
                    execute_network_actions(app, actions);
                    return; // state already dropped — skip the implicit drop
                }
            }
        }
        SocialMessage::PartyDecline { .. } => {
            if state.social.party.consume_outgoing_invite(&authenticated_sender) {
                let _ = app.emit(
                    "party_invite_declined",
                    serde_json::json!({ "fromHash": sender_hex }),
                );
            }
        }

        // ── Self-authenticating: leave ──────────────────────────────────
        SocialMessage::PartyLeave { from } => {
            if let Some(ref mut party) = state.social.party.party {
                if !party.is_member(&from) {
                    return;
                }
                let (_remaining, new_leader) = party.remove_member(&from);
                let dissolving = party.members.len() <= 1;
                if dissolving {
                    state.social.party.party = None;
                    state.social.party.clear_outgoing_invites();
                } else if let Some(leader) = new_leader {
                    let _ = app.emit(
                        "party_leader_changed",
                        serde_json::json!({ "newLeaderHash": hex::encode(leader) }),
                    );
                }
                let _ = app.emit(
                    "party_member_left",
                    serde_json::json!({ "memberHash": sender_hex }),
                );
                if dissolving {
                    let _ = app.emit("party_dissolved", serde_json::json!({}));
                }
            }
        }

        // ── 3. Leader-authorised party control ──────────────────────────
        SocialMessage::PartyKick { target, .. } => {
            let party = match state.social.party.party.as_mut() {
                Some(p) => p,
                None => return, // not in a party — ignore
            };
            if !party.is_leader(&authenticated_sender) {
                return; // only the leader can kick
            }
            let dissolved;
            if target == our_address {
                state.social.party.party = None;
                state.social.party.clear_outgoing_invites();
                dissolved = true;
            } else if party.is_member(&target) {
                party.remove_member(&target);
                if party.members.len() <= 1 {
                    state.social.party.party = None;
                    state.social.party.clear_outgoing_invites();
                    dissolved = true;
                } else {
                    dissolved = false;
                }
            } else {
                return; // target not in our party
            }
            let _ = app.emit(
                "party_kick",
                serde_json::json!({ "targetHash": hex::encode(target) }),
            );
            if dissolved {
                let _ = app.emit("party_dissolved", serde_json::json!({}));
            }
        }
        SocialMessage::PartyMemberJoined {
            member,
            display_name,
            ..
        } => {
            let now = now_secs(app);
            // Self-confirmation: leader confirmed our join request.
            // Verify the sender is the leader we expect from our pending join.
            let is_our_join = member == our_address
                && state
                    .social
                    .party
                    .pending_join
                    .as_ref()
                    .is_some_and(|pj| pj.leader == authenticated_sender);
            if is_our_join {
                if state.social.party.confirm_join(our_address, now).is_ok() {
                    let _ = app.emit(
                        "party_joined",
                        serde_json::json!({
                            "leaderHash": hex::encode(authenticated_sender),
                        }),
                    );
                }
                return;
            }
            // Normal path: another member joined our existing party.
            if let Some(ref mut party) = state.social.party.party {
                if !party.is_leader(&authenticated_sender) {
                    return;
                }
                if party.add_member(social::party::PartyMember {
                    address_hash: member,
                    display_name: display_name.clone(),
                    joined_at: now,
                }).is_ok() {
                    let _ = app.emit(
                        "party_member_joined",
                        serde_json::json!({
                            "memberHash": hex::encode(member),
                            "memberName": display_name,
                        }),
                    );
                }
            }
        }
        SocialMessage::PartyMemberLeft { member, .. } => {
            if let Some(ref mut party) = state.social.party.party {
                if !party.is_leader(&authenticated_sender) && authenticated_sender != member {
                    return; // only leader or the departing member can notify
                }
                if !party.is_member(&member) {
                    return;
                }
                let (_remaining, new_leader) = party.remove_member(&member);
                let dissolving = party.members.len() <= 1;
                if dissolving {
                    state.social.party.party = None;
                    state.social.party.clear_outgoing_invites();
                } else if let Some(leader) = new_leader {
                    let _ = app.emit(
                        "party_leader_changed",
                        serde_json::json!({ "newLeaderHash": hex::encode(leader) }),
                    );
                }
                let _ = app.emit(
                    "party_member_left",
                    serde_json::json!({ "memberHash": hex::encode(member) }),
                );
                if dissolving {
                    let _ = app.emit("party_dissolved", serde_json::json!({}));
                }
            }
        }
        SocialMessage::PartyDissolved { .. } => {
            if let Some(ref party) = state.social.party.party {
                if !party.is_leader(&authenticated_sender) {
                    return; // only leader can dissolve
                }
            } else {
                return; // not in a party
            }
            state.social.party.party = None;
            state.social.party.clear_outgoing_invites();
            let _ = app.emit("party_dissolved", serde_json::json!({}));
        }
        SocialMessage::PartyLeaderChanged { new_leader, .. } => {
            if let Some(ref mut party) = state.social.party.party {
                if !party.is_leader(&authenticated_sender) {
                    return; // only current leader can transfer
                }
                if !party.is_member(&new_leader) {
                    return; // new leader must be a member
                }
                party.leader = new_leader;
                let _ = app.emit(
                    "party_leader_changed",
                    serde_json::json!({ "newLeaderHash": hex::encode(new_leader) }),
                );
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
            if trade_mgr
                .receive_accept(trade_id, &authenticated_sender, now_secs(app))
                .is_ok()
            {
                let _ = app.emit("trade_event", serde_json::json!({"type": "accepted"}));
            }
        }
        TradeMessage::Decline { trade_id, .. } => {
            if trade_mgr
                .receive_decline(trade_id, &authenticated_sender)
                .is_ok()
            {
                let _ = app.emit("trade_event", serde_json::json!({"type": "declined"}));
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
            if let Ok(both_locked) = trade_mgr.receive_remote_lock(
                trade_id,
                &authenticated_sender,
                terms_hash,
                now_secs(app),
            ) {
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
                    let mut guard = state_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());
                    let state = &mut *guard;
                    match trade_mgr.execute_trade(
                        &mut state.inventory,
                        &mut state.currants,
                        &state.item_defs,
                    ) {
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
                            let _ =
                                app.emit("trade_event", serde_json::json!({"type": "completed"}));
                            // Send Complete courtesy message to peer + record trust signal.
                            let net = app.state::<NetworkWrapper>();
                            let mut ns = net.0.lock().unwrap_or_else(|e| e.into_inner());
                            ns.trust_store.record_trade_success(&authenticated_sender);
                            let actions = ns.send_trade_message(
                                &complete_msg,
                                &authenticated_sender,
                                &mut rand::rngs::OsRng,
                            );
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
                            // No trust penalty — execute_trade failures are local
                            // (insufficient items/currants), not remote peer misbehavior.
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
            if trade_mgr
                .receive_remote_unlock(trade_id, &authenticated_sender, now_secs(app))
                .is_ok()
            {
                let _ = app.emit(
                    "trade_event",
                    serde_json::json!({"type": "unlocked", "who": "remote"}),
                );
            }
        }
        TradeMessage::Cancel { trade_id, .. } => {
            if trade_mgr
                .receive_cancel(trade_id, &authenticated_sender)
                .is_ok()
            {
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

/// Unix epoch seconds for timestamping persisted group ops. Unlike `now_secs`
/// (process-uptime), this survives restart and is comparable across peers —
/// necessary for `joined_at` tenure tracking and for DAG tie-breakers.
fn group_op_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Validate and normalize a group name on the backend. The UI enforces these
/// rules too, but the IPC layer must not trust client input.
fn normalize_group_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Group name must not be empty".to_string());
    }
    const MAX_GROUP_NAME_CHARS: usize = 40;
    if trimmed.chars().count() > MAX_GROUP_NAME_CHARS {
        return Err(format!(
            "Group name must be {MAX_GROUP_NAME_CHARS} characters or fewer"
        ));
    }
    Ok(trimmed.to_string())
}

/// Validate that the player is within interact_radius of the given entity.
fn validate_entity_proximity(
    state: &engine::state::GameState,
    entity_id: &str,
) -> Result<(), String> {
    let entity = state
        .world_entities
        .iter()
        .find(|e| e.id == entity_id)
        .ok_or_else(|| format!("Unknown entity: {entity_id}"))?;
    let def = state.entity_defs.get(&entity.entity_type);
    let radius = def.map(|d| d.interact_radius).unwrap_or(60.0);
    let dx = state.player.x - entity.x;
    let dy = state.player.y - entity.y;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist > radius {
        return Err("Too far".to_string());
    }
    Ok(())
}

#[tauri::command]
fn jukebox_play(entity_id: String, app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    validate_entity_proximity(&state, &entity_id)?;
    if let Some(jb) = state.jukebox_states.get_mut(&entity_id) {
        jb.play();
    }
    Ok(())
}

#[tauri::command]
fn jukebox_pause(entity_id: String, app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    validate_entity_proximity(&state, &entity_id)?;
    if let Some(jb) = state.jukebox_states.get_mut(&entity_id) {
        jb.pause();
    }
    Ok(())
}

#[tauri::command]
fn jukebox_select_track(
    entity_id: String,
    track_index: usize,
    app: AppHandle,
) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    validate_entity_proximity(&state, &entity_id)?;
    if let Some(jb) = state.jukebox_states.get_mut(&entity_id) {
        jb.select_track(track_index);
    }
    Ok(())
}

#[tauri::command]
fn get_jukebox_state(entity_id: String, app: AppHandle) -> Result<serde_json::Value, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    let entity = state
        .world_entities
        .iter()
        .find(|e| e.id == entity_id)
        .ok_or_else(|| format!("Unknown entity: {entity_id}"))?;
    let def = state.entity_defs.get(&entity.entity_type);
    let name = def.map(|d| d.name.as_str()).unwrap_or("Jukebox");

    let jb = state.jukebox_states.get(&entity_id);
    let playlist: Vec<serde_json::Value> = jb
        .map(|jb| {
            jb.playlist
                .iter()
                .map(|track_id| {
                    let track_def = state.track_catalog.tracks.get(track_id);
                    serde_json::json!({
                        "id": track_id,
                        "title": track_def.map(|t| t.title.as_str()).unwrap_or("Unknown"),
                        "artist": track_def.map(|t| t.artist.as_str()).unwrap_or("Unknown"),
                        "durationSecs": track_def.map(|t| t.duration_secs).unwrap_or(0.0),
                    })
                })
                .collect()
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
    let entity = state
        .world_entities
        .iter()
        .find(|e| e.id == entity_id)
        .ok_or_else(|| format!("Unknown entity: {entity_id}"))?;
    let def = state
        .entity_defs
        .get(&entity.entity_type)
        .ok_or_else(|| format!("Unknown entity type: {}", entity.entity_type))?;
    let store_id = def
        .store
        .as_ref()
        .ok_or_else(|| format!("Entity '{}' is not a vendor", entity.entity_type))?;
    let store = state
        .store_catalog
        .stores
        .get(store_id)
        .ok_or_else(|| format!("Unknown store: {store_id}"))?;

    let vendor_inventory: Vec<serde_json::Value> = store
        .inventory
        .iter()
        .filter_map(|item_id| {
            let item_def = state.item_defs.get(item_id)?;
            let base_cost = item_def.base_cost?;
            Some(serde_json::json!({
                "itemId": item_id,
                "name": item_def.name,
                "baseCost": base_cost,
                "stackLimit": item_def.stack_limit,
            }))
        })
        .collect();

    // Build player sellable inventory: deduplicate by item_id
    let mut seen = std::collections::HashMap::<String, u32>::new();
    for stack in state.inventory.slots.iter().flatten() {
        *seen.entry(stack.item_id.clone()).or_insert(0) += stack.count;
    }
    let mut player_inventory: Vec<serde_json::Value> = seen
        .iter()
        .filter_map(|(item_id, &count)| {
            let sell = item::vendor::sell_price(item_id, &state.item_defs, store)?;
            let item_def = state.item_defs.get(item_id)?;
            Some(serde_json::json!({
                "itemId": item_id,
                "name": item_def.name,
                "count": count,
                "sellPrice": sell,
            }))
        })
        .collect();
    // Sort by item name for stable display order across refreshes
    player_inventory.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
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
fn vendor_buy(
    entity_id: String,
    item_id: String,
    count: u32,
    app: AppHandle,
) -> Result<u64, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    validate_entity_proximity(&state, &entity_id)?;

    let entity = state
        .world_entities
        .iter()
        .find(|e| e.id == entity_id)
        .ok_or_else(|| format!("Unknown entity: {entity_id}"))?;
    let def = state
        .entity_defs
        .get(&entity.entity_type)
        .ok_or_else(|| format!("Unknown entity type: {}", entity.entity_type))?;
    let store_id = def
        .store
        .as_ref()
        .ok_or_else(|| format!("Entity '{}' is not a vendor", entity.entity_type))?;
    let store = state
        .store_catalog
        .stores
        .get(store_id)
        .ok_or_else(|| format!("Unknown store: {store_id}"))?
        .clone();
    let item_defs = state.item_defs.clone();

    let currants = state.currants;
    let discount = item::imagination::haggling_discount(state.upgrades.haggling_tier);
    let new_balance = item::vendor::buy(
        &item_id,
        count,
        currants,
        &mut state.inventory,
        &item_defs,
        &store,
        discount,
    )?;
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
        color: None,
    });

    Ok(new_balance)
}

#[tauri::command]
fn vendor_sell(
    entity_id: String,
    item_id: String,
    count: u32,
    app: AppHandle,
) -> Result<u64, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    validate_entity_proximity(&state, &entity_id)?;

    let entity = state
        .world_entities
        .iter()
        .find(|e| e.id == entity_id)
        .ok_or_else(|| format!("Unknown entity: {entity_id}"))?;
    let def = state
        .entity_defs
        .get(&entity.entity_type)
        .ok_or_else(|| format!("Unknown entity type: {}", entity.entity_type))?;
    let store_id = def
        .store
        .as_ref()
        .ok_or_else(|| format!("Entity '{}' is not a vendor", entity.entity_type))?;
    let store = state
        .store_catalog
        .stores
        .get(store_id)
        .ok_or_else(|| format!("Unknown store: {store_id}"))?
        .clone();
    let item_defs = state.item_defs.clone();

    let old_balance = state.currants;
    let new_balance = item::vendor::sell(
        &item_id,
        count,
        old_balance,
        &mut state.inventory,
        &item_defs,
        &store,
    )?;
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
        color: None,
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

    let (new_energy, new_max, mood_gained) = item::energy::eat(
        &item_id,
        energy,
        max_energy,
        &mut state.inventory,
        &item_defs,
    )?;
    state.energy = new_energy;
    if mood_gained > 0.0 {
        state.social.mood.apply_mood_change(mood_gained);
    }

    // Apply buff effect (if item has one).
    if let Some(item_def) = state.item_defs.get(&item_id).cloned() {
        let gt = state.game_time;
        crate::buff::apply_item_buff(&mut state.social.buffs, &item_def, gt);
    }

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
        color: None,
    });
    if mood_gained > 0.0 {
        let fb_id = state.next_feedback_id;
        state.next_feedback_id += 1;
        state.pickup_feedback.push(item::types::PickupFeedback {
            id: fb_id,
            text: format!("+{} mood", mood_gained.round() as u32),
            success: true,
            x: px,
            y: py,
            age_secs: 0.0,
            color: Some("#c084fc".to_string()),
        });
    }

    Ok(serde_json::json!({
        "energy": new_energy,
        "maxEnergy": new_max,
    }))
}

#[tauri::command]
fn get_upgrade_defs() -> Vec<serde_json::Value> {
    vec![
        serde_json::to_value(&item::imagination::ENERGY_TANK).unwrap(),
        serde_json::to_value(&item::imagination::HAGGLING).unwrap(),
    ]
}

#[tauri::command]
fn buy_upgrade(upgrade_id: String, app: AppHandle) -> Result<serde_json::Value, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    let mut imagination = state.imagination;
    let mut upgrades = state.upgrades.clone();
    let result = item::imagination::buy_upgrade(&upgrade_id, &mut imagination, &mut upgrades)?;
    state.imagination = imagination;
    state.upgrades = upgrades;

    let px = state.player.x;
    let py = state.player.y;

    match result {
        item::imagination::UpgradeEffect::EnergyTankDelta(delta) => {
            state.max_energy += delta;
            state.energy += delta;
            let fb_id = state.next_feedback_id;
            state.next_feedback_id += 1;
            state.pickup_feedback.push(item::types::PickupFeedback {
                id: fb_id,
                text: format!("+{} max energy!", delta as u32),
                success: true,
                x: px,
                y: py,
                age_secs: 0.0,
                color: Some("#4ade80".to_string()),
            });
        }
        item::imagination::UpgradeEffect::HagglingDiscount(discount) => {
            let fb_id = state.next_feedback_id;
            state.next_feedback_id += 1;
            state.pickup_feedback.push(item::types::PickupFeedback {
                id: fb_id,
                text: format!("Haggling → {}%", (discount * 100.0).round() as u32),
                success: true,
                x: px,
                y: py,
                age_secs: 0.0,
                color: Some("#fbbf24".to_string()),
            });
        }
    }

    Ok(serde_json::json!({
        "imagination": state.imagination,
        "upgrades": state.upgrades,
        "energy": state.energy,
        "maxEnergy": state.max_energy,
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
        let peer = mgr
            .active_peer_hash()
            .ok_or("No active trade after accept")?;
        (msg, peer)
    };
    send_trade_msg(&app, &msg, &peer_hash);
    // Emit accepted event so the responder's own UI transitions from prompt to trade panel.
    let _ = app.emit("trade_event", serde_json::json!({"type": "accepted"}));
    Ok(())
}

#[tauri::command]
fn trade_decline(app: AppHandle) -> Result<(), String> {
    let (msg, peer_hash) = {
        let trade = app.state::<TradeWrapper>();
        let mut mgr = trade.0.lock().map_err(|e| e.to_string())?;
        let peer = mgr
            .pending_peer_hash()
            .ok_or("No pending trade to decline")?;
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
                    eprintln!(
                        "[trade] Retaining journal — save failed, trade recoverable on restart"
                    );
                }
                drop(guard);
                drop(mgr);
                // Defer network sends until after all locks are released.
                send_trade_msg(&app, &lock_msg, &peer_hash);
                send_trade_msg(&app, &complete_msg, &peer_hash);
                let _ = app.emit("trade_event", serde_json::json!({"type": "completed"}));
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

            // 7b. Annotate remote players with social state + compute nearest social target
            {
                let state_wrapper = app.state::<GameStateWrapper>();
                let mut game_state = state_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());

                let mut nearest_target: Option<(f64, engine::state::NearestSocialTarget)> = None;
                let mut buddy_addrs: Vec<[u8; 16]> = Vec::new();

                for rp in &mut frame.remote_players {
                    if let Ok(bytes) = hex::decode(&rp.address_hash) {
                        if bytes.len() == 16 {
                            let mut addr = [0u8; 16];
                            addr.copy_from_slice(&bytes);
                            rp.is_buddy = game_state.social.buddies.is_buddy(&addr);
                            if rp.is_buddy {
                                buddy_addrs.push(addr);
                            }
                            let mut in_party = false;
                            if let Some(ref party) = game_state.social.party.party {
                                rp.party_role = party.role_of(&addr).map(|r| {
                                    in_party = true;
                                    match r {
                                        crate::social::party::PartyRole::Leader => "Leader".to_string(),
                                        crate::social::party::PartyRole::Member => "Member".to_string(),
                                    }
                                });
                            }

                            // Compute distance for nearest social target
                            let dx = frame.player.x - rp.x;
                            let dy = frame.player.y - rp.y;
                            let dist = (dx * dx + dy * dy).sqrt();
                            const SOCIAL_INTERACTION_RADIUS: f64 = 400.0;
                            if dist <= SOCIAL_INTERACTION_RADIUS {
                                let is_closer = nearest_target.as_ref().is_none_or(|(d, _)| dist < *d);
                                if is_closer {
                                    nearest_target = Some((dist, engine::state::NearestSocialTarget {
                                        address_hash: rp.address_hash.clone(),
                                        display_name: rp.display_name.clone(),
                                        is_buddy: rp.is_buddy,
                                        in_party,
                                    }));
                                }
                            }
                        }
                    }
                }

                // Feed copresence for online buddies
                if !buddy_addrs.is_empty() {
                    let today = crate::date_util::today_date_string();
                    for addr in &buddy_addrs {
                        game_state.social.buddies.record_copresence(addr, dt, &today);
                    }
                }

                frame.nearest_social_target = nearest_target.map(|(_, t)| t);
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

fn load_street_xml(name: &str, app: &AppHandle) -> Result<String, String> {
    // Demo streets are always available via compile-time embedding.
    match name {
        "demo_meadow" | "LADEMO001" => {
            return Ok(include_str!("../../assets/streets/demo_meadow.xml").to_string());
        }
        "demo_heights" | "LADEMO002" => {
            return Ok(include_str!("../../assets/streets/demo_heights.xml").to_string());
        }
        _ => {}
    }

    // Look up imported streets by TSID in the manifest.
    let manifest = app.state::<StreetManifestState>();
    let entry = manifest
        .0
        .streets
        .get(name)
        .ok_or_else(|| format!("Unknown street: {name}"))?;
    let streets_dir = app.state::<StreetsDir>();
    if entry.filename.contains("..")
        || entry.filename.contains('/')
        || entry.filename.contains('\\')
    {
        return Err(format!(
            "Manifest filename for {name} contains unsafe path components"
        ));
    }
    let path = streets_dir.0.join(&entry.filename);
    std::fs::read_to_string(path).map_err(|e| format!("Failed to read street {name}: {e}"))
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

    #[test]
    fn validate_group_active_rejects_dissolved() {
        use crate::social::groups::GroupManager;
        use harmony_groups::{GroupAction, GroupMode, GroupOp};

        let dir = tempfile::TempDir::new().unwrap();
        let mut mgr = GroupManager::new(dir.path().to_path_buf());

        let founder: [u8; 16] = [0x11; 16];
        let group_id: [u8; 16] = [0xCD; 16];

        // Create the group — active state passes the guard.
        let (create_op, _) = GroupOp::new_unsigned(
            vec![],
            founder,
            1_700_000_000,
            GroupAction::Create {
                group_id,
                name: "Doomed".to_string(),
                mode: GroupMode::InviteOnly,
            },
        );
        mgr.merge_op(group_id, create_op).unwrap();
        let active = mgr.get_state(group_id).unwrap();
        assert!(validate_group_active(active).is_ok());

        // Dissolve the group — guard must reject subsequent mutations.
        let parents = mgr.head_ops(group_id);
        let (dissolve_op, _) = GroupOp::new_unsigned(
            parents,
            founder,
            1_700_000_001,
            GroupAction::Dissolve,
        );
        mgr.merge_op(group_id, dissolve_op).unwrap();
        let dissolved = mgr.get_state(group_id).unwrap();
        assert!(dissolved.dissolved, "sanity: state should be dissolved");
        let err = validate_group_active(dissolved).expect_err("dissolved must error");
        assert!(
            err.contains("dissolved"),
            "error message should mention dissolved, got: {err}"
        );
    }
}

#[cfg(test)]
mod emote_fire_tests {
    use super::*;
    use crate::emote::{EmoteKind, EmoteKindTag, EmoteState};
    use crate::mood::MoodState;
    use std::time::{Duration, Instant};

    fn id(seed: u8) -> [u8; 16] {
        [seed; 16]
    }

    #[test]
    fn fire_emote_success_applies_sender_mood_for_dance() {
        let mut emotes = EmoteState::new(id(0x01), "2026-04-10");
        let mut mood = MoodState::default();
        mood.mood = 50.0; // below max so the +2.0 delta has headroom
        let initial = mood.mood;
        let now = Instant::now();

        let result = fire_emote(
            &mut emotes,
            &mut mood,
            id(0x01),
            &EmoteKind::Dance,
            None,
            now,
        );

        assert!(matches!(result, EmoteFireResult::Success { .. }));
        assert!((mood.mood - (initial + 2.0)).abs() < 0.01);
    }

    #[test]
    fn fire_emote_success_carries_cooldown_ms() {
        // Dance has no per-pair cooldown — Success should report the
        // global anti-mash (2s).
        let mut emotes = EmoteState::new(id(0x01), "2026-04-10");
        let mut mood = MoodState::default();
        let result = fire_emote(
            &mut emotes, &mut mood, id(0x01), &EmoteKind::Dance, None, Instant::now(),
        );
        match result {
            EmoteFireResult::Success { cooldown_ms } => {
                assert_eq!(cooldown_ms, 2000, "dance should return global cooldown");
            }
            other => panic!("expected Success, got {:?}", other),
        }
    }

    #[test]
    fn fire_emote_hug_success_carries_per_pair_cooldown_ms() {
        // Hug's per-pair cooldown (60s) dominates the global (2s).
        let mut emotes = EmoteState::new(id(0x01), "2026-04-10");
        let mut mood = MoodState::default();
        let result = fire_emote(
            &mut emotes, &mut mood, id(0x01), &EmoteKind::Hug, Some(id(0x02)), Instant::now(),
        );
        match result {
            EmoteFireResult::Success { cooldown_ms } => {
                assert_eq!(cooldown_ms, 60_000, "hug should return per-pair 60s");
            }
            other => panic!("expected Success, got {:?}", other),
        }
    }

    #[test]
    fn fire_emote_reward_cooldown_blocks_second_mood_but_not_fire() {
        let mut emotes = EmoteState::new(id(0x01), "2026-04-10");
        let mut mood = MoodState::default();
        let t0 = Instant::now();

        let _ = fire_emote(&mut emotes, &mut mood, id(0x01), &EmoteKind::Dance, None, t0);
        let after_first = mood.mood;

        // Past global fire cooldown but within reward window
        let t1 = t0 + Duration::from_secs(3);
        let result = fire_emote(&mut emotes, &mut mood, id(0x01), &EmoteKind::Dance, None, t1);
        assert!(matches!(result, EmoteFireResult::Success { .. }));
        // Fire succeeded, but mood was NOT credited (reward cooldown)
        assert!((mood.mood - after_first).abs() < 0.01);
    }

    #[test]
    fn fire_emote_returns_cooldown_when_global_active() {
        let mut emotes = EmoteState::new(id(0x01), "2026-04-10");
        let mut mood = MoodState::default();
        let t0 = Instant::now();
        let _ = fire_emote(&mut emotes, &mut mood, id(0x01), &EmoteKind::Dance, None, t0);
        let t1 = t0 + Duration::from_millis(500);
        let result = fire_emote(&mut emotes, &mut mood, id(0x01), &EmoteKind::Dance, None, t1);
        match result {
            EmoteFireResult::Cooldown { remaining_ms } => {
                assert!(remaining_ms > 1400 && remaining_ms <= 1500);
            }
            other => panic!("expected Cooldown, got {:?}", other),
        }
    }

    #[test]
    fn fire_emote_hug_requires_target() {
        let mut emotes = EmoteState::new(id(0x01), "2026-04-10");
        let mut mood = MoodState::default();
        let result = fire_emote(
            &mut emotes, &mut mood, id(0x01), &EmoteKind::Hug, None, Instant::now(),
        );
        assert!(matches!(result, EmoteFireResult::NoTarget));
    }

    #[test]
    fn fire_emote_wave_broadcast_succeeds_per_spec() {
        // Spec §1: Wave is "Targeted or broadcast" — a broadcast wave
        // (casual greeting to the street) is valid. Sender mood is gated
        // by the 30s reward cooldown, not by target presence.
        let mut emotes = EmoteState::new(id(0x01), "2026-04-10");
        let mut mood = MoodState::default();
        let result = fire_emote(
            &mut emotes, &mut mood, id(0x01), &EmoteKind::Wave, None, Instant::now(),
        );
        assert!(matches!(result, EmoteFireResult::Success { .. }));
    }

    #[test]
    fn fire_emote_high_five_requires_target() {
        let mut emotes = EmoteState::new(id(0x01), "2026-04-10");
        let mut mood = MoodState::default();
        let result = fire_emote(
            &mut emotes, &mut mood, id(0x01), &EmoteKind::HighFive, None, Instant::now(),
        );
        assert!(matches!(result, EmoteFireResult::NoTarget));
    }

    #[test]
    fn fire_emote_hug_applies_sender_mood_plus_five() {
        let mut emotes = EmoteState::new(id(0x01), "2026-04-10");
        let mut mood = MoodState::default();
        mood.mood = 50.0; // below max so the +5.0 delta has headroom
        let initial = mood.mood;
        let result = fire_emote(
            &mut emotes, &mut mood, id(0x01), &EmoteKind::Hug, Some(id(0x02)), Instant::now(),
        );
        assert!(matches!(result, EmoteFireResult::Success { .. }));
        assert!((mood.mood - (initial + 5.0)).abs() < 0.01);
    }

    #[test]
    fn fire_emote_hug_per_pair_cooldown_blocks_same_target_within_60s() {
        let mut emotes = EmoteState::new(id(0x01), "2026-04-10");
        let mut mood = MoodState::default();
        let t0 = Instant::now();
        let _ = fire_emote(&mut emotes, &mut mood, id(0x01), &EmoteKind::Hug, Some(id(0x02)), t0);
        let t1 = t0 + Duration::from_secs(30); // past global, within per-pair
        let result = fire_emote(&mut emotes, &mut mood, id(0x01), &EmoteKind::Hug, Some(id(0x02)), t1);
        assert!(matches!(result, EmoteFireResult::Cooldown { .. }));
    }

    #[test]
    fn hi_daily_cap_prevents_second_hi_to_same_target_same_day() {
        // Hi has its own per-day gate (can_hi) that should survive the fire_emote
        // refactor — migrated emote_hi wrapper must still enforce it.
        let mut emotes = EmoteState::new(id(0x01), "2026-04-10");
        let target = id(0x02);

        // Simulate emote_hi's own daily-gate check (which lives in the wrapper):
        assert!(emotes.can_hi(&target));
        emotes.record_hi_sent(target);
        assert!(!emotes.can_hi(&target));
    }

    #[test]
    fn receive_emote_applies_target_mood_for_hug() {
        let me = id(0x01);
        let them = id(0x02);
        let mut emotes = EmoteState::new(me, "2026-04-10");
        let mut mood = MoodState::default();
        mood.mood = 50.0; // leave headroom for +5 (default is 100.0 = clamp max)
        let initial = mood.mood;

        let delta = apply_receive_mood(
            &mut emotes,
            &mut mood,
            them,
            &EmoteKind::Hug,
            true, false, Instant::now(),
        );

        assert!((delta - 5.0).abs() < 0.01);
        assert!((mood.mood - (initial + 5.0)).abs() < 0.01);
    }

    #[test]
    fn receive_emote_applies_witness_mood_for_dance_when_nearby() {
        let me = id(0x01);
        let them = id(0x02);
        let mut emotes = EmoteState::new(me, "2026-04-10");
        let mut mood = MoodState::default();
        mood.mood = 50.0;
        let initial = mood.mood;

        let delta = apply_receive_mood(
            &mut emotes, &mut mood, them, &EmoteKind::Dance,
            false, true, Instant::now(),
        );

        assert!((delta - 1.0).abs() < 0.01);
        assert!((mood.mood - (initial + 1.0)).abs() < 0.01);
    }

    #[test]
    fn receive_emote_no_mood_for_dance_when_not_nearby() {
        let mut emotes = EmoteState::new(id(0x01), "2026-04-10");
        let mut mood = MoodState::default();
        mood.mood = 50.0;
        let initial = mood.mood;

        let delta = apply_receive_mood(
            &mut emotes, &mut mood, id(0x02), &EmoteKind::Dance,
            false, false, Instant::now(),
        );

        assert!((delta - 0.0).abs() < 0.01);
        assert!((mood.mood - initial).abs() < 0.01);
    }

    #[test]
    fn receive_emote_reward_cooldown_blocks_second_dance_mood_from_same_dancer() {
        let me = id(0x01);
        let them = id(0x02);
        let mut emotes = EmoteState::new(me, "2026-04-10");
        let mut mood = MoodState::default();
        mood.mood = 50.0;
        let t0 = Instant::now();

        let first = apply_receive_mood(
            &mut emotes, &mut mood, them, &EmoteKind::Dance, false, true, t0,
        );
        assert!((first - 1.0).abs() < 0.01);
        let after_first = mood.mood;

        let t1 = t0 + Duration::from_secs(60);
        let second = apply_receive_mood(
            &mut emotes, &mut mood, them, &EmoteKind::Dance, false, true, t1,
        );
        assert!((second - 0.0).abs() < 0.01);
        assert!((mood.mood - after_first).abs() < 0.01);
    }

    #[test]
    fn receive_emote_applaud_broadcast_gives_witness_mood() {
        let me = id(0x01);
        let them = id(0x02);
        let mut emotes = EmoteState::new(me, "2026-04-10");
        let mut mood = MoodState::default();
        mood.mood = 50.0;
        let initial = mood.mood;

        let delta = apply_receive_mood(
            &mut emotes, &mut mood, them, &EmoteKind::Applaud,
            false, true, Instant::now(),
        );

        assert!((delta - 3.0).abs() < 0.01);
        assert!((mood.mood - (initial + 3.0)).abs() < 0.01);
    }

    #[test]
    fn receive_emote_applaud_targeted_at_us_gives_target_mood() {
        let me = id(0x01);
        let them = id(0x02);
        let mut emotes = EmoteState::new(me, "2026-04-10");
        let mut mood = MoodState::default();
        mood.mood = 50.0;
        let initial = mood.mood;

        let delta = apply_receive_mood(
            &mut emotes, &mut mood, them, &EmoteKind::Applaud,
            true, false, Instant::now(),
        );

        assert!((delta - 3.0).abs() < 0.01);
        assert!((mood.mood - (initial + 3.0)).abs() < 0.01);
    }

    #[test]
    fn privacy_toggle_round_trip_on_state() {
        let mut s = EmoteState::new(id(0x01), "2026-04-10");
        assert_eq!((s.accept_hug, s.accept_high_five), (true, true));

        s.set_privacy(EmoteKindTag::Hug, false);
        assert_eq!((s.accept_hug, s.accept_high_five), (false, true));

        s.set_privacy(EmoteKindTag::HighFive, false);
        assert_eq!((s.accept_hug, s.accept_high_five), (false, false));

        s.set_privacy(EmoteKindTag::Hug, true);
        assert_eq!((s.accept_hug, s.accept_high_five), (true, false));
    }

    #[test]
    fn is_target_in_range_accepts_close_target() {
        assert!(is_target_in_range(0.0, 0.0, 100.0, 100.0, 400.0));
    }

    #[test]
    fn is_target_in_range_rejects_far_target() {
        assert!(!is_target_in_range(0.0, 0.0, 500.0, 0.0, 400.0));
    }

    #[test]
    fn is_target_in_range_at_boundary_accepts_equal_distance() {
        // Exactly at boundary should accept (matches emote_hi's `dist <= 400.0` semantics)
        assert!(is_target_in_range(0.0, 0.0, 400.0, 0.0, 400.0));
    }

    // ── effective_receive_target ────────────────────────────────────────
    //
    // Defense-in-depth: broadcast-only kinds must be coerced to broadcast
    // on receive regardless of what the sender wired.

    #[test]
    fn effective_receive_target_strips_dance_target() {
        let msg = crate::emote::EmoteMessage {
            kind: crate::emote::EmoteKind::Dance,
            target: Some(id(0x02)),
        };
        assert_eq!(
            effective_receive_target(&msg),
            None,
            "targeted Dance must be coerced to broadcast"
        );
    }

    #[test]
    fn effective_receive_target_preserves_hug_target() {
        let msg = crate::emote::EmoteMessage {
            kind: crate::emote::EmoteKind::Hug,
            target: Some(id(0x02)),
        };
        assert_eq!(effective_receive_target(&msg), Some(id(0x02)));
    }

    #[test]
    fn effective_receive_target_preserves_applaud_target() {
        // Applaud is dual-nature (targeted OR witness) — target must survive.
        let msg = crate::emote::EmoteMessage {
            kind: crate::emote::EmoteKind::Applaud,
            target: Some(id(0x02)),
        };
        assert_eq!(effective_receive_target(&msg), Some(id(0x02)));
    }

    #[test]
    fn effective_receive_target_passes_through_none() {
        let msg = crate::emote::EmoteMessage {
            kind: crate::emote::EmoteKind::Dance,
            target: None,
        };
        assert_eq!(effective_receive_target(&msg), None);
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
            let skill_defs = skill::loader::parse_skill_defs(
                include_str!("../../assets/skills.json")
            ).expect("Failed to parse skills.json");
            // Validate skill definitions
            for (skill_id, skill_def) in &skill_defs {
                for prereq in &skill_def.prerequisites {
                    assert!(
                        skill_defs.contains_key(prereq),
                        "Skill '{skill_id}' references unknown prerequisite '{prereq}'"
                    );
                }
                for recipe_id in &skill_def.unlocks_recipes {
                    assert!(
                        recipe_defs.contains_key(recipe_id),
                        "Skill '{skill_id}' references unknown recipe '{recipe_id}'"
                    );
                }
            }
            let dialogue_defs = quest::loader::parse_dialogue_defs(
                include_str!("../../assets/dialogues.json")
            ).expect("Failed to parse dialogues.json");
            let quest_defs = quest::loader::parse_quest_defs(
                include_str!("../../assets/quests.json")
            ).expect("Failed to parse quests.json");
            // Validate quest turn-in NPC and deliver objective npc_id refs
            for (quest_id, quest_def) in &quest_defs {
                let npc_type = &quest_def.turn_in_npc;
                assert!(
                    entity_defs.get(npc_type).and_then(|d| d.dialogue.as_ref()).is_some(),
                    "Quest '{quest_id}' turn_in_npc '{npc_type}' is not a dialogue entity"
                );
                for obj in &quest_def.objectives {
                    if let quest::types::QuestObjective::Deliver { npc_id, .. } = obj {
                        assert!(
                            entity_defs.get(npc_id).and_then(|d| d.dialogue.as_ref()).is_some(),
                            "Quest '{quest_id}' deliver objective npc_id '{npc_id}' is not a dialogue entity"
                        );
                    }
                }
            }
            // Validate dialogue tree refs in entity_defs exist in dialogue_defs
            for (entity_id, entity_def) in &entity_defs {
                if let Some(tree_id) = &entity_def.dialogue {
                    assert!(
                        dialogue_defs.contains_key(tree_id),
                        "Entity '{entity_id}' references unknown dialogue tree '{tree_id}'"
                    );
                }
            }
            // Validate completeQuest effects only appear in the correct NPC's dialogue tree.
            // Build reverse map: dialogue tree_id → entity type that owns it.
            let tree_to_entity: std::collections::HashMap<&str, &str> = entity_defs
                .iter()
                .filter_map(|(eid, def)| def.dialogue.as_deref().map(|tid| (tid, eid.as_str())))
                .collect();
            for (_tree_id, tree) in &dialogue_defs {
                for node in tree.nodes.values() {
                    for option in &node.options {
                        for effect in &option.effects {
                            if let quest::types::DialogueEffect::CompleteQuest { quest_id } = effect {
                                if let Some(qdef) = quest_defs.get(quest_id) {
                                    if let Some(&owner_entity) = tree_to_entity.get(_tree_id.as_str()) {
                                        assert!(
                                            owner_entity == qdef.turn_in_npc,
                                            "completeQuest '{quest_id}' in dialogue tree '{_tree_id}' \
                                             (owned by '{owner_entity}') but quest turn_in_npc is '{}'",
                                            qdef.turn_in_npc
                                        );
                                    }
                                }
                            }
                        }
                    }
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
                skill_defs,
                quest_defs,
                dialogue_defs,
            )))
        })
        .manage(InputStateWrapper(Mutex::new(InputState::default())))
        .manage(GameRunning(Mutex::new(false)))
        .manage(GameLoopHandle(Mutex::new(None)))
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;

            // Streets directory: env override or default under app data dir
            let streets_dir = std::env::var("HARMONY_STREETS_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| data_dir.join("streets"));
            let manifest = street::manifest::StreetManifest::load(&streets_dir.join("manifest.json"));
            if !manifest.streets.is_empty() {
                println!("[streets] Loaded manifest with {} imported streets", manifest.streets.len());
            }
            app.manage(StreetsDir(streets_dir));
            app.manage(StreetManifestState(manifest));

            let kits_dir = data_dir.join("sound-kits");
            if let Err(e) = std::fs::create_dir_all(&kits_dir) {
                eprintln!("[sound-kits] Failed to create {}: {e}", kits_dir.display());
            }
            app.manage(SoundKitsDir(kits_dir));
            let (player_identity, identity_proof, display_name, setup_complete) =
                identity::persistence::load_or_create_profile(
                    &data_dir,
                    &harmony_identity::PuzzleParams::PRODUCTION,
                )
                .map_err(std::io::Error::other)?;

            // Save identity bytes BEFORE moving identity into PlayerIdentityWrapper,
            // since PrivateIdentity is not Clone.
            let identity_bytes = zeroize::Zeroizing::new(player_identity.to_private_bytes());

            app.manage(PlayerIdentityWrapper {
                identity: Mutex::new(player_identity),
                identity_proof,
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
                identity_proof,
                harmony_identity::PuzzleParams::PRODUCTION,
            ))));
            app.manage(TradeWrapper(Mutex::new(trade::state::TradeManager::new(our_hash))));

            // Bind UDP transport for LAN discovery.
            let transport = UdpTransport::bind(DEFAULT_PORT)
                .map_err(|e| std::io::Error::other(format!("UDP bind failed: {e}")))?;
            app.manage(TransportWrapper(Mutex::new(transport)));

            let _ = std::fs::create_dir_all(data_dir.join("groups"));
            let mut group_mgr = crate::social::groups::GroupManager::new(data_dir.clone());

            // Rebuild pending invites from persisted op logs so invites that
            // arrived in a previous session survive restart. We do NOT emit
            // `group_invite_received` events here — the frontend has not yet
            // registered listeners at setup time, so events would be lost.
            // Instead, the frontend calls `get_pending_invites` on mount to
            // drain the rebuilt state.
            let _ = group_mgr.rebuild_pending_invites(our_hash, 0.0);
            app.manage(GroupManagerWrapper(Mutex::new(group_mgr)));

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
            get_upgrade_defs,
            buy_upgrade,
            trade_initiate,
            trade_accept,
            trade_decline,
            trade_update_offer,
            trade_lock,
            trade_unlock,
            trade_cancel,
            trade_get_state,
            get_skills,
            learn_skill,
            cancel_learning,
            get_dialogue_state,
            dialogue_choose,
            close_dialogue,
            get_quest_log,
            get_mood,
            emote_hi,
            emote,
            set_emote_privacy,
            get_emote_privacy,
            buddy_request,
            buddy_accept,
            buddy_decline,
            buddy_remove,
            block_player,
            unblock_player,
            get_buddy_list,
            get_blocked_list,
            party_invite,
            party_accept,
            party_decline,
            party_leave,
            party_kick,
            get_party_state,
            group_create,
            group_invite,
            group_accept,
            group_decline,
            group_join,
            group_leave,
            group_kick,
            group_promote,
            group_demote,
            group_dissolve,
            group_update_info,
            get_group_state,
            get_my_groups,
            get_pending_invites,
        ])
        .run(tauri::generate_context!())
        .expect("error while running harmony-glitch");
}
