pub mod commands;
pub mod models;
pub mod ssh;
pub mod store;

use std::sync::Arc;

use tauri::Manager;

use crate::ssh::SshManager;
use crate::store::Store;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let store = Arc::new(Store::from_app(&handle)?);
            let manager = Arc::new(SshManager::new(handle, Arc::clone(&store)));
            app.manage(store);
            app.manage(manager);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_sessions,
            commands::save_session,
            commands::delete_session,
            commands::get_settings,
            commands::save_settings,
            commands::data_dir,
            commands::ssh_connect,
            commands::ssh_disconnect,
            commands::ssh_connected,
            commands::ssh_sessions,
            commands::list_known_hosts,
            commands::ssh_write,
            commands::ssh_resize,
            commands::ssh_exec,
            commands::batch_exec,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            if let Some(manager) = app_handle.try_state::<Arc<SshManager>>() {
                let manager = Arc::clone(manager.inner());
                tauri::async_runtime::block_on(async move {
                    manager.disconnect_all().await;
                });
            }
        }
    });
}
