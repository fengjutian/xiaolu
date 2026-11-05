mod system;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![system::get_system_snapshot])
        .run(tauri::generate_context!())
        .expect("failed to run System Lens");
}
