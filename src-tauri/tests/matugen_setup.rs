//! End to end with the real matugen (skipped when it is not installed): set-up
//! in a sandboxed config folder, one matugen run, and the theme reads the palette.
use std::process::Command;

#[test]
fn setup_then_one_matugen_run_gives_a_readable_palette() {
    if Command::new("matugen").arg("--version").output().is_err() {
        eprintln!("matugen not installed; skipped");
        return;
    }
    let root = std::env::temp_dir().join(format!("oumu-matugen-{}", std::process::id()));
    let (config, cache) = (root.join("config"), root.join("cache"));
    std::fs::create_dir_all(config.join("matugen")).unwrap();
    std::fs::write(config.join("matugen/config.toml"), "[config]\n").unwrap();
    // A sandbox: no Ryoku palette, matugen's own config under the temp root.
    std::env::set_var("XDG_CONFIG_HOME", &config);
    std::env::set_var("XDG_CACHE_HOME", &cache);

    let before = oumu_lib::matugen::status("");
    assert!(!before.ryoku && !before.registered && !before.palette_ready);
    let after = oumu_lib::matugen::setup("").expect("set-up");
    assert!(after.registered);
    assert!(std::fs::read_to_string(config.join("matugen/config.toml.before-oumu")).unwrap().starts_with("[config]"));

    let out = Command::new("matugen")
        .args(["color", "hex", "#6750a4", "--mode", "dark"])
        .env("XDG_CONFIG_HOME", &config)
        .output()
        .unwrap();
    assert!(out.status.success(), "matugen: {}", String::from_utf8_lossy(&out.stderr));
    let st = oumu_lib::matugen::status("");
    assert!(st.palette_ready, "palette at {}", st.palette_path);
    let pal = oumu_lib::theme::read_palette(std::path::Path::new(&st.palette_path)).unwrap();
    assert!(pal.contains_key("primary") && pal.contains_key("surfaceContainerLow") && pal.contains_key("onSurface"));
    let _ = std::fs::remove_dir_all(&root);
}
