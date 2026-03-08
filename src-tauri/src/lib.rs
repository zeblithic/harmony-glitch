pub mod avatar;
pub mod engine;
pub mod physics;
pub mod street;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to Ur.", name)
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running harmony-glitch");
}
