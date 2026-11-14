// The app icon inside the exe (Explorer, taskbar, Start menu, Add/Remove Programs).
fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    embed_resource::compile("app.rc", embed_resource::NONE).manifest_optional().unwrap();
}
