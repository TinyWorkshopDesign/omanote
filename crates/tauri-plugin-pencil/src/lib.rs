//! The sketch canvas as a native PencilKit view on iPadOS.
//!
//! A web canvas only sees pointer events, so it never gets the Apple Pencil Pro's
//! squeeze, double tap, barrel roll or haptics, nor PencilKit's own inks and
//! erasers. On iOS this plugin lays a transparent `PKCanvasView` over the area the
//! web toolbar leaves free; the toolbar stays in the web view and drives it through
//! the plugin commands, and the drawing comes back as the same PNG the web canvas
//! makes. On other platforms the plugin does nothing and the web canvas is used.

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_pencil);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("pencil")
        .setup(|_app, _api| {
            #[cfg(target_os = "ios")]
            _api.register_ios_plugin(init_plugin_pencil)?;
            Ok(())
        })
        .build()
}
