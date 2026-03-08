pub mod avatar;
pub mod engine;
pub mod physics;
pub mod street;

use engine::state::GameState;
use physics::movement::InputState;
use street::parser::parse_street;
use street::types::StreetData;

use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Shared game state protected by a mutex.
struct GameStateWrapper(Mutex<GameState>);

/// Shared input state — written by frontend, read by game loop.
struct InputStateWrapper(Mutex<InputState>);

/// Flag to control the game loop.
struct GameRunning(Mutex<bool>);

#[tauri::command]
fn list_streets() -> Vec<String> {
    // For Phase A: return hardcoded demo street names.
    // Later: scan assets directory or query content network.
    vec!["demo_meadow".to_string()]
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

    let app_handle = app.clone();
    std::thread::spawn(move || {
        game_loop(app_handle);
    });

    Ok(())
}

#[tauri::command]
fn stop_game(app: AppHandle) -> Result<(), String> {
    let running = app.state::<GameRunning>();
    let mut is_running = running.0.lock().map_err(|e| e.to_string())?;
    *is_running = false;
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

        // Tick game state
        let state_wrapper = app.state::<GameStateWrapper>();
        let mut state = state_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(frame) = state.tick(dt, &input) {
            drop(state); // Release lock before emitting
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
    // Phase A: load from bundled assets directory
    // The asset files live at assets/streets/<name>.xml relative to the app
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../assets/streets")
        .join(format!("{}.xml", name));

    std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to load street '{}': {} (path: {:?})", name, e, path))
}

pub fn run() {
    tauri::Builder::default()
        .manage(GameStateWrapper(Mutex::new(GameState::new(1280.0, 720.0))))
        .manage(InputStateWrapper(Mutex::new(InputState::default())))
        .manage(GameRunning(Mutex::new(false)))
        .invoke_handler(tauri::generate_handler![
            list_streets,
            load_street,
            send_input,
            start_game,
            stop_game,
        ])
        .run(tauri::generate_context!())
        .expect("error while running harmony-glitch");
}
