// The app icon inside the exe (Explorer, taskbar, Start menu, Add/Remove Programs).
fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    embed_resource::compile("app.rc", embed_resource::NONE).manifest_optional().unwrap();
}
