// src/tray.rs
// System tray con menú estilo Capri conectado a AvatarSystem y OpenClawBridge

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, Runtime,
};

pub fn setup_tray<R: Runtime>(app_handle: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    let show_hide = MenuItem::with_id(
        app_handle,
        "show_hide",
        "Mostrar / Ocultar a Fuko",
        true,
        None::<&str>,
    )?;
    let chat = MenuItem::with_id(
        app_handle,
        "chat",
        "Hablar con ella",
        true,
        None::<&str>,
    )?;
    let analyze_screen = MenuItem::with_id(
        app_handle,
        "analyze_screen",
        "Mirar mi pantalla ahora",
        true,
        None::<&str>,
    )?;

    let separator1 = PredefinedMenuItem::separator(app_handle)?;

    let sit_on_window = MenuItem::with_id(
        app_handle,
        "sit_on_window",
        "Sentarse / Levantarse",
        true,
        None::<&str>,
    )?;
    let wander = MenuItem::with_id(app_handle, "wander", "Caminar ahora", true, None::<&str>)?;
    let dance = MenuItem::with_id(app_handle, "dance", "Bailar", true, None::<&str>)?;
    let glitch = MenuItem::with_id(app_handle, "glitch", "Teletransportarse", true, None::<&str>)?;

    let separator2 = PredefinedMenuItem::separator(app_handle)?;

    let mainframe = MenuItem::with_id(
        app_handle,
        "mainframe",
        "Mainframe (Cerebro & Carpetas)",
        true,
        None::<&str>,
    )?;

    let separator3 = PredefinedMenuItem::separator(app_handle)?;

    let quit = MenuItem::with_id(app_handle, "quit", "Salir de Fuko", true, None::<&str>)?;

    let menu = Menu::with_items(
        app_handle,
        &[
            &show_hide,
            &chat,
            &analyze_screen,
            &separator1,
            &sit_on_window,
            &wander,
            &dance,
            &glitch,
            &separator2,
            &mainframe,
            &separator3,
            &quit,
        ],
    )?;

    let mut tray_builder = TrayIconBuilder::with_id("claudia-tray").menu(&menu);
    if let Some(icon) = app_handle.default_window_icon() {
        tray_builder = tray_builder.icon(icon.clone());
    }
    let tray = tray_builder.build(app_handle)?;
    app_handle.manage(tray);

    let app_handle_clone = app_handle.clone();
    app_handle_clone.on_menu_event(move |app, event| {
        match event.id().as_ref() {
            "show_hide" => {
                if let Some(window) = app.get_webview_window("overlay_window") {
                    let visible = window.is_visible().unwrap_or(false);
                    let _ = crate::overlay_window::toggle_window(&window, !visible);
                }
            }
            "chat" => {
                if let Some(window) = app.get_webview_window("overlay_window") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                let _ = app.emit("open_chat_input", true);
            }
            "analyze_screen" => {
                let _ = app.emit("trigger_analyze_notebook", true);
            }
            "sit_on_window" => {
                let _ = app.emit("capri_action", "toggle_sit");
            }
            "wander" => {
                let _ = app.emit("capri_action", "walk_now");
            }
            "dance" => {
                let _ = app.emit("capri_action", "toggle_dance");
            }
            "glitch" => {
                let _ = app.emit("capri_action", "teleport_random");
            }
            "mainframe" => {
                let _ = crate::overlay_window::open_mainframe(app);
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        }
    });

    Ok(())
}
