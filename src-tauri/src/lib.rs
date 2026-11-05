mod system;
mod sensors;
mod drivers;
mod benchmark;
mod diagnostics;
mod network_diagnostics;

use tauri::{menu::{Menu, MenuItem}, tray::TrayIconBuilder, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(std::sync::Mutex::new(system::SystemCollector::new().expect("failed to initialize system collector")))
        .manage(benchmark::BenchmarkHistoryLock(std::sync::Mutex::new(())))
        .setup(|app| {
            let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("app icon").clone())
                .tooltip("小鲁 · 系统透镜")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => if let Some(window) = app.get_webview_window("main") { let _ = window.show(); let _ = window.set_focus(); },
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, _| {
                    if let Some(window) = tray.app_handle().get_webview_window("main") { let _ = window.show(); let _ = window.set_focus(); }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![system::get_system_snapshot, system::get_hardware_details, system::get_startup_items, system::set_startup_enabled, sensors::get_sensor_snapshot, drivers::get_driver_inventory, drivers::scan_driver_updates, benchmark::run_benchmark, benchmark::get_benchmark_history, benchmark::save_external_benchmark, diagnostics::get_diagnostic_report, network_diagnostics::get_network_diagnostics])
        .run(tauri::generate_context!())
        .expect("failed to run System Lens");
}
