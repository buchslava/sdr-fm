use tauri::menu::{AboutMetadata, Menu, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Result, Runtime};

pub fn default_menu<R: Runtime>(app: &AppHandle<R>) -> Result<Menu<R>> {
    let config = app.config();
    let name = config
        .product_name
        .clone()
        .unwrap_or_else(|| app.package_info().name.clone());

    let about_label = format!("About {name}");
    let hide_label = format!("Hide {name}");
    let quit_label = format!("Quit {name}");

    let about_metadata = AboutMetadata {
        name: Some(name.clone()),
        version: Some(app.package_info().version.to_string()),
        copyright: config.bundle.copyright.clone(),
        license: config
            .bundle
            .license
            .clone()
            .or_else(|| Some("MIT License".into())),
        authors: Some(vec![
            "Viacheslav Chub (viacheslav.chub@gmail.com)".into(),
        ]),
        ..Default::default()
    };

    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    let help_menu = Submenu::with_items(app, "Help", true, &[])?;

    Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                &name,
                true,
                &[
                    &PredefinedMenuItem::about(app, Some(&about_label), Some(about_metadata))?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::services(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::hide(app, Some(&hide_label))?,
                    &PredefinedMenuItem::hide_others(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::quit(app, Some(&quit_label))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                "File",
                true,
                &[&PredefinedMenuItem::close_window(app, None)?],
            )?,
            &Submenu::with_items(
                app,
                "Edit",
                true,
                &[
                    &PredefinedMenuItem::undo(app, None)?,
                    &PredefinedMenuItem::redo(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, None)?,
                    &PredefinedMenuItem::copy(app, None)?,
                    &PredefinedMenuItem::paste(app, None)?,
                    &PredefinedMenuItem::select_all(app, None)?,
                ],
            )?,
            &Submenu::with_items(
                app,
                "View",
                true,
                &[&PredefinedMenuItem::fullscreen(app, None)?],
            )?,
            &window_menu,
            &help_menu,
        ],
    )
}
