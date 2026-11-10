//! Windows backends: UI Automation text access, and a clipboard paste for apps
//! whose fields cannot be edited through it.

pub mod apply;
pub mod autostart;
pub mod ax;
pub mod clip;
pub mod focus;
pub mod remote;
pub mod startup;
