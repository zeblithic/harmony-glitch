pub mod avatar;
pub mod engine;
pub mod identity;
pub mod network;
pub mod physics;
pub mod street;

use engine::state::GameState;
use physics::movement::InputState;
use street::parser::parse_street;
use street::types::StreetData;

use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

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
    data_dir: std::path::PathBuf,
}

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
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    state.load_street(street_data.clone());

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
            game_loop(app_handle);
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
    Ok(serde_json::json!({
        "displayName": *name,
        "addressHash": hex::encode(identity.public_identity().address_hash),
    }))
}

#[tauri::command]
fn set_display_name(name: String, app: AppHandle) -> Result<(), String> {
    let pi = app.state::<PlayerIdentityWrapper>();
    let mut display_name = pi.display_name.lock().map_err(|e| e.to_string())?;
    *display_name = name.clone();

    // Persist to disk so the name survives restarts.
    let identity = pi.identity.lock().map_err(|e| e.to_string())?;
    let profile = identity::persistence::PlayerProfile {
        identity_hex: hex::encode(identity.to_private_bytes()),
        display_name: name,
    };
    let json = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;
    identity::persistence::write_profile(&pi.data_dir.join("profile.json"), &json)?;
    Ok(())
}

fn game_loop(app: AppHandle) {
    let tick_duration = Duration::from_secs_f64(1.0 / 60.0);
    let dt = 1.0 / 60.0;

    loop {
        let tick_start = Instant::now();

        // Check if still running
        let running = app.state::<GameRunning>();
        let is_running = running.0.lock().unwrap_or_else(|e| e.into_inner());
        if !*is_running {
            break;
        }
        drop(is_running);

        // Read current input
        let input_wrapper = app.state::<InputStateWrapper>();
        let input = *input_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());

        // Tick game state — lock is scoped to the block so it is always
        // released before emitting, preventing potential deadlocks.
        let frame = {
            let state_wrapper = app.state::<GameStateWrapper>();
            let mut state = state_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());
            state.tick(dt, &input)
        };

        if let Some(frame) = frame {
            let _ = app.emit("render_frame", &frame);
        }

        // Sleep for remainder of tick
        let elapsed = tick_start.elapsed();
        if elapsed < tick_duration {
            std::thread::sleep(tick_duration - elapsed);
        }
    }
}

fn load_street_xml(name: &str) -> Result<String, String> {
    // Phase A: embed street XML at compile time so the binary is self-contained
    // and doesn't depend on CARGO_MANIFEST_DIR paths at runtime.
    match name {
        "demo_meadow" => Ok(include_str!("../../assets/streets/demo_meadow.xml").to_string()),
        "demo_heights" => Ok(include_str!("../../assets/streets/demo_heights.xml").to_string()),
        _ => Err(format!("Unknown street: {}", name)),
    }
}

pub fn run() {
    tauri::Builder::default()
        .manage(GameStateWrapper(Mutex::new(GameState::new(1280.0, 720.0))))
        .manage(InputStateWrapper(Mutex::new(InputState::default())))
        .manage(GameRunning(Mutex::new(false)))
        .manage(GameLoopHandle(Mutex::new(None)))
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let (player_identity, display_name) =
                identity::persistence::load_or_create_profile(&data_dir)
                    .map_err(std::io::Error::other)?;
            app.manage(PlayerIdentityWrapper {
                identity: Mutex::new(player_identity),
                display_name: Mutex::new(display_name),
                data_dir: data_dir.clone(),
            });
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running harmony-glitch");
}
