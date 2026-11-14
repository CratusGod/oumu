// The setup exes carry the release builds: build `-p utsuri -p oumu --release` first.
// Without them (a fresh clone running `cargo build` or `cargo test` on the whole workspace)
// they embed an empty placeholder, so the workspace still builds, and such a setup refuses to
// run and says what to build first.
fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/release");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    for (var, exe) in [("UTSURI_PAYLOAD", "utsuri.exe"), ("OUMU_PAYLOAD", "oumu.exe")] {
        let p = root.join(exe);
        println!("cargo:rerun-if-changed={}", p.display());
        let p = if p.is_file() {
            p
        } else {
            println!("cargo:warning={exe} isn't built yet: this setup won't install anything. Run `cargo build --release -p utsuri -p oumu` first, then build `-p setup` again.");
            let empty = out.join(format!("missing-{exe}"));
            std::fs::write(&empty, b"").unwrap();
            empty
        };
        println!("cargo:rustc-env={var}={}", p.display());
    }
    embed_resource::compile_for("utsuri-setup.rc", ["utsuri-setup"], embed_resource::NONE).manifest_optional().unwrap();
    embed_resource::compile_for("oumu-setup.rc", ["oumu-setup"], embed_resource::NONE).manifest_optional().unwrap();
}
