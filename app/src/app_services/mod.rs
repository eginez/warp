//! Functionality relating to services that the application provides
//! to the host system.
//!
//! For example, on macOS, this module sets up integrations with
//! Finder such that the user can open a new Warp tab or window
//! in a given directory.

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub mod linux;
#[cfg(target_os = "macos")]
mod mac;
pub(crate) mod terminal_control_service;
#[cfg(windows)]
pub mod windows;

use warpui::AppContext;

pub fn init(ctx: &mut AppContext) {
    log::info!("Initializing app services");
    ctx.add_singleton_model(terminal_control_service::TerminalControlServiceHost::new);

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    linux::init(ctx);
    #[cfg(target_os = "macos")]
    {
        let _ = ctx;
        mac::init();
    }
    #[cfg(windows)]
    windows::init(ctx);
}

pub fn teardown(ctx: &mut AppContext) {
    log::info!("Tearing down app services...");

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    linux::teardown(ctx);
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    let _ = ctx;
}
