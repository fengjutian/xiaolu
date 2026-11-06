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
mod cleanup_exec;
mod storage_health;
mod event_log;
mod nvme_health;
mod system_history;
mod toolbox;
mod toolbox_host;
mod notifications;
mod app_settings;
mod launcher;
mod desktop_quick;
mod devices;
mod shell;
mod eye_care;
mod ai;

use tauri::{menu::{Menu, MenuItem}, tray::TrayIconBuilder, Manager, RunEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(std::sync::Mutex::new(system::SystemCollector::new().expect("failed to initialize system collector")))
        .manage(benchmark::BenchmarkHistoryLock(std::sync::Mutex::new(())))
        .manage(benchmark::BenchmarkCancel::default())
        .manage(cleanup_exec::CleanupPlanState::default())
        .manage(notifications::NotificationRuntime::default())
        .manage(ai::AiState::default())
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
            eye_care::restore_on_startup(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 开发模式下关闭窗口即退出进程。否则上一次 `tauri dev` 的实例会
                // 留在托盘里继续占用 WebView2 用户数据目录，导致下一次启动时
                // 创建 WebView 失败（HRESULT 0x8007139F / 0x800700AA）。
                if cfg!(debug_assertions) {
                    let _ = api;
                    return;
                }
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![system::get_system_snapshot, system::get_process_inventory, system::get_system_history, toolbox_host::list_toolbox_tools, toolbox_host::run_tool_action, system::get_hardware_details, system::get_startup_items, system::set_startup_enabled, sensors::get_sensor_snapshot, sensors::get_sensor_history, sensors::get_sensor_settings, sensors::save_sensor_settings, drivers::get_driver_inventory, drivers::scan_driver_updates, drivers::assess_driver_updates, benchmark::run_benchmark, benchmark::cancel_benchmark, benchmark::benchmark_preflight, benchmark::get_benchmark_history, benchmark::save_external_benchmark, diagnostics::get_diagnostic_report, network_diagnostics::get_network_diagnostics, software::get_software_inventory, storage_analysis::scan_storage, storage_analysis::scan_large_files, storage_analysis::scan_duplicates, storage_analysis::scan_directory, storage_analysis::get_storage_drives, scheduled_tasks::get_scheduled_tasks, cleanup_preview::scan_cleanup_candidates, cleanup_exec::prepare_cleanup, cleanup_exec::execute_cleanup, event_log::query_event_log, nvme_health::get_nvme_health, notifications::get_notification_settings, notifications::save_notification_settings, notifications::notify_sensor_alert, notifications::send_test_notification, notifications::send_break_reminder, devices::get_device_details, shell::open_path_in_explorer, shell::open_recycle_bin, eye_care::get_eye_care_settings, eye_care::save_eye_care_settings, eye_care::set_eye_care_filter, app_settings::get_app_settings, app_settings::save_app_settings, app_settings::get_app_paths, system_history::clear_system_history, software::launch_software_uninstall, launcher::list_start_menu_apps, launcher::launch_start_menu_entry, desktop_quick::get_desktop_overview, desktop_quick::open_desktop_item, ai::get_ai_settings, ai::save_ai_settings, ai::clear_ai_api_key, ai::build_ai_snapshot, ai::preview_ai_payload, ai::ai_analyze, ai::cancel_ai_request, ai::get_ai_sessions, ai::delete_ai_session, ai::clear_ai_sessions, ai::set_ai_session_feedback, ai::get_ai_tool_audit, ai::clear_ai_tool_audit, ai::preview_ai_repair, ai::probe_ai_capabilities])
        .build(tauri::generate_context!())
        .expect("failed to build System Lens")
        .run(|_app, event| {
            if let RunEvent::Exit = event {
                eye_care::shutdown();
            }
        });
}
