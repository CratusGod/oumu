//! Linux backends: AT-SPI text access, Hyprland geometry and IPC, the primary selection,
//! the `oumu --translate` socket, and XDG autostart and logging.

pub mod apply;
pub mod autostart;
pub mod ax;
pub mod desktop;
pub mod hyprland;
pub mod remote;
pub mod selection;
pub mod startup;
