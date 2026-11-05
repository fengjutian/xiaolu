mod system;
mod sensors;
mod drivers;
mod benchmark;
mod diagnostics;
mod network_diagnostics;
mod software;
mod storage_analysis;
mod scheduled_tasks;
mod cleanup_preview;
mod storage_health;
mod event_log;
mod nvme_health;
mod system_history;
mod toolbox;

use tauri::{menu::{Menu, MenuItem}, tray::TrayIconBuilder, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(std::sync::Mutex::new(system::SystemCollector::new().expect("failed to initialize system collector")))
        .manage(benchmark::BenchmarkHistoryLock(std::sync::Mutex::new(())))
        .manage(benchmark::BenchmarkCancel::default())
        .setup(|app| {
            let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            TrayIconBuilder::with_id("main")
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
        .invoke_handler(tauri::generate_handler![system::get_system_snapshot, system::get_system_history, toolbox::list_toolbox_tools, toolbox::scan_c_drive_slim, system::get_hardware_details, system::get_startup_items, system::set_startup_enabled, sensors::get_sensor_snapshot, sensors::get_sensor_history, sensors::get_sensor_settings, sensors::save_sensor_settings, drivers::get_driver_inventory, drivers::scan_driver_updates, drivers::assess_driver_updates, benchmark::run_benchmark, benchmark::cancel_benchmark, benchmark::benchmark_preflight, benchmark::get_benchmark_history, benchmark::save_external_benchmark, diagnostics::get_diagnostic_report, network_diagnostics::get_network_diagnostics, software::get_software_inventory, storage_analysis::scan_storage, storage_analysis::scan_large_files, storage_analysis::scan_duplicates, storage_analysis::scan_directory, scheduled_tasks::get_scheduled_tasks, cleanup_preview::scan_cleanup_candidates, event_log::query_event_log, nvme_health::get_nvme_health])
        .run(tauri::generate_context!())
        .expect("failed to run System Lens");
}
