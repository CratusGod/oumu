//! Per-user install and uninstall for Utsuri and Oumu (no admin rights): the exe in
//! %LOCALAPPDATA%\Programs\<App>, a Start menu shortcut, start at sign-in, and an entry in
//! Settings → Apps whose Uninstall runs `<App>.exe --uninstall`.
//!
//! `suffix` (e.g. " (test)") and `dir` make a sandboxed install for tests: every name it
//! writes carries the suffix, so the real install and the old apps are never touched.
#![cfg(windows)]

use anyhow::{Context as _, Result, bail};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy)]
pub struct AppInfo {
    pub name: &'static str,
    pub exe: &'static str,
    pub version: &'static str,
    pub publisher: &'static str,
    /// Arguments for the start-at-sign-in entry.
    pub autostart_args: &'static str,
    /// Uninstall keys of older builds (the Python Utsuri, the Tauri Oumu).
    pub old_keys: &'static [&'static str],
}

pub const UTSURI: AppInfo = AppInfo {
    name: "Utsuri",
    exe: "Utsuri.exe",
    version: "2.0.0",
    publisher: "CratusGod",
    autostart_args: "--quiet",
    old_keys: &["{9910EB1A-E5FE-43F9-B79D-1CAF25C3AF14}_is1"],
};

pub const OUMU: AppInfo = AppInfo { name: "Oumu", exe: "Oumu.exe", version: "0.2.0", publisher: "CratusGod", autostart_args: "", old_keys: &["Oumu"] };

#[derive(Clone, Debug, Default)]
pub struct Opts {
    /// Install folder (default %LOCALAPPDATA%\Programs\<App><suffix>).
    pub dir: Option<PathBuf>,
    /// Added to every name written (tests).
    pub suffix: String,
    /// No questions, no message boxes.
    pub silent: bool,
    pub autostart: bool,
    pub launch: bool,
    /// Install even when an older build is still installed (tests).
    pub ignore_old: bool,
    /// Silent too: run the old uninstaller without asking.
    pub remove_old: bool,
    /// Tests: look for this uninstall key instead of the real old ones.
    pub old_key: Option<String>,
}

impl Opts {
    /// --dir <path>, --suffix <text>, --silent, --no-autostart, --launch, --ignore-old
    pub fn from_args(args: &[String]) -> Opts {
        let mut o = Opts { autostart: true, ..Default::default() };
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--dir" if i + 1 < args.len() => {
                    o.dir = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
                "--suffix" if i + 1 < args.len() => {
                    o.suffix = args[i + 1].clone();
                    i += 1;
                }
                "--silent" | "/S" | "/s" => o.silent = true,
                "--no-autostart" => o.autostart = false,
                "--launch" => o.launch = true,
                "--ignore-old" => o.ignore_old = true,
                "--remove-old" => o.remove_old = true,
                "--old-key" if i + 1 < args.len() => {
                    o.old_key = Some(args[i + 1].clone());
                    i += 1;
                }
                _ => {}
            }
            i += 1;
        }
        o
    }
}

fn key_name(app: &AppInfo, o: &Opts) -> String {
    format!("{}{}", app.name, o.suffix.replace(|c: char| !c.is_ascii_alphanumeric(), ""))
}

fn display(app: &AppInfo, o: &Opts) -> String {
    format!("{}{}", app.name, o.suffix)
}

pub fn install_dir(app: &AppInfo, o: &Opts) -> PathBuf {
    o.dir.clone().unwrap_or_else(|| {
        let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        base.join("Programs").join(display(app, o))
    })
}

fn start_menu_link(app: &AppInfo, o: &Opts) -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(r"Microsoft\Windows\Start Menu\Programs").join(format!("{}.lnk", display(app, o))))
}

// ---------------------------------------------------------------- dialogs

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// Yes/No question; true = Yes.
pub fn ask(title: &str, text: &str) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::*;
    let (t, m) = (wide(title), wide(text));
    unsafe { MessageBoxW(None, windows::core::PCWSTR(m.as_ptr()), windows::core::PCWSTR(t.as_ptr()), MB_YESNO | MB_ICONQUESTION | MB_SETFOREGROUND) == IDYES }
}

pub fn tell(title: &str, text: &str, error: bool) {
    use windows::Win32::UI::WindowsAndMessaging::*;
    let (t, m) = (wide(title), wide(text));
    let icon = if error { MB_ICONERROR } else { MB_ICONINFORMATION };
    unsafe {
        MessageBoxW(None, windows::core::PCWSTR(m.as_ptr()), windows::core::PCWSTR(t.as_ptr()), MB_OK | icon | MB_SETFOREGROUND);
    }
}

// ---------------------------------------------------------------- processes

/// Running processes started from `exe` (full path, case-insensitive), except this one.
fn processes_of(exe: &Path) -> Vec<u32> {
    let want = exe.to_string_lossy().to_lowercase();
    all_processes().into_iter().filter(|(_, p)| *p == want).map(|(pid, _)| pid).collect()
}

/// (pid, lowercase exe path) of every process we can see, except this one.
fn all_processes() -> Vec<(u32, String)> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::*;
    use windows::Win32::System::Threading::*;
    let me = std::process::id();
    let mut out = vec![];
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            if e.th32ProcessID != me {
                if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, e.th32ProcessID) {
                    let mut buf = [0u16; 1024];
                    let mut n = buf.len() as u32;
                    if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).is_ok() {
                        out.push((e.th32ProcessID, String::from_utf16_lossy(&buf[..n as usize]).to_lowercase()));
                    }
                    let _ = CloseHandle(h);
                }
            }
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

/// Stop every copy of `exe` (they hold the file open).
fn stop(exe: &Path) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::*;
    for pid in processes_of(exe) {
        unsafe {
            if let Ok(h) = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid) {
                let _ = TerminateProcess(h, 0);
                let _ = WaitForSingleObject(h, 3000);
                let _ = CloseHandle(h);
            }
        }
    }
}

// ---------------------------------------------------------------- shortcuts

fn create_shortcut(lnk: &Path, target: &Path, desc: &str) -> Result<()> {
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{HSTRING, Interface};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&HSTRING::from(target.as_os_str()))?;
        if let Some(d) = target.parent() {
            link.SetWorkingDirectory(&HSTRING::from(d.as_os_str()))?;
        }
        link.SetDescription(&HSTRING::from(desc))?;
        link.SetIconLocation(&HSTRING::from(target.as_os_str()), 0)?;
        let pf: IPersistFile = link.cast()?;
        if let Some(d) = lnk.parent() {
            std::fs::create_dir_all(d)?;
        }
        pf.Save(&HSTRING::from(lnk.as_os_str()), true)?;
    }
    Ok(())
}

// ---------------------------------------------------------------- old builds

/// (key, display name, uninstall command) of an older build still installed.
pub fn old_install(app: &AppInfo, o: &Opts) -> Option<(String, String, String)> {
    let hk = RegKey::predef(HKEY_CURRENT_USER);
    let keys: Vec<String> = match &o.old_key {
        Some(k) => vec![k.clone()],
        None => app.old_keys.iter().map(|s| s.to_string()).collect(),
    };
    for k in &keys {
        let Ok(key) = hk.open_subkey(format!(r"{UNINSTALL}\{k}")) else { continue };
        let cmd: String = key.get_value("UninstallString").unwrap_or_default();
        // our own entry (same key name, our exe) is not "old"
        if cmd.contains("--uninstall") {
            continue;
        }
        // its uninstaller is gone: a dead entry left in Settings → Apps
        if !uninstaller_exe(&cmd).is_some_and(|p| p.exists()) {
            drop(key);
            let _ = hk.delete_subkey_all(format!(r"{UNINSTALL}\{k}"));
            continue;
        }
        let name: String = key.get_value("DisplayName").unwrap_or_else(|_| app.name.to_string());
        let ver: String = key.get_value("DisplayVersion").unwrap_or_default();
        return Some((k.to_string(), format!("{name} {ver}").trim().to_string(), cmd));
    }
    None
}

/// The exe an UninstallString runs.
fn uninstaller_exe(cmd: &str) -> Option<PathBuf> {
    let cmd = cmd.trim();
    let p = if let Some(rest) = cmd.strip_prefix('"') {
        rest.split('"').next()?.to_string()
    } else {
        let i = cmd.to_lowercase().find(".exe")?;
        cmd[..i + 4].to_string()
    };
    Some(PathBuf::from(p))
}

/// Run the old build's own uninstaller and wait until it is really done. NSIS and Inno
/// uninstallers copy themselves to %TEMP% and relaunch, so the first process returns at
/// once while the real one still shows its dialog: wait for those copies too.
fn run_old_uninstaller(key: &str, cmd: &str) -> Result<()> {
    let temp = std::env::temp_dir().to_string_lossy().to_lowercase();
    let in_temp = || -> Vec<u32> { all_processes().into_iter().filter(|(_, p)| p.starts_with(&temp)).map(|(pid, _)| pid).collect() };
    let before: std::collections::HashSet<u32> = in_temp().into_iter().collect();
    std::process::Command::new("cmd").raw_arg(format!("/C \"{cmd}\"")).creation_flags(NO_WINDOW).status()?;
    let hk = RegKey::predef(HKEY_CURRENT_USER);
    let gone = || hk.open_subkey(format!(r"{UNINSTALL}\{key}")).is_err();
    let start = std::time::Instant::now();
    let mut idle_since = std::time::Instant::now();
    // up to 15 minutes: the user may take their time in the old uninstaller's dialog
    while start.elapsed() < std::time::Duration::from_secs(900) {
        if in_temp().iter().any(|p| !before.contains(p)) {
            idle_since = std::time::Instant::now();
        } else if gone() || idle_since.elapsed() > std::time::Duration::from_secs(3) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    if !gone() {
        if uninstaller_exe(cmd).is_some_and(|p| p.exists()) {
            bail!("the previous version wasn't removed");
        }
        // files gone but the entry left behind
        let _ = hk.delete_subkey_all(format!(r"{UNINSTALL}\{key}"));
    }
    Ok(())
}

// ---------------------------------------------------------------- install

/// Install `payload` (the app's exe). Returns the installed exe.
pub fn install(app: &AppInfo, payload: &[u8], o: &Opts, after: impl FnOnce(&Path) -> Result<()>) -> Result<PathBuf> {
    let title = format!("{} setup", display(app, o));
    // built before the app itself (see crates/setup/build.rs): nothing to install
    anyhow::ensure!(!payload.is_empty(), "this setup was built without {0}. Build it with `cargo build --release -p {1}`, then `cargo build --release -p setup` again.", app.exe, app.exe.trim_end_matches(".exe").to_lowercase());
    if !o.ignore_old {
        if let Some((key, name, cmd)) = old_install(app, o) {
            if o.silent && !o.remove_old {
                bail!("{name} is still installed: remove it first (or pass --remove-old)");
            }
            if !o.silent && !ask(&title, &format!("{name} (the previous version) is installed.\n\nRemove it now and install {} {}?", app.name, app.version)) {
                bail!("cancelled: {name} is still installed");
            }
            run_old_uninstaller(&key, &cmd)?;
        }
    }
    let dir = install_dir(app, o);
    std::fs::create_dir_all(&dir).with_context(|| format!("couldn't create {}", dir.display()))?;
    let exe = dir.join(app.exe);
    stop(&exe);
    let tmp = dir.join(format!("{}.new", app.exe));
    std::fs::write(&tmp, payload).with_context(|| format!("couldn't write {}", tmp.display()))?;
    std::fs::rename(&tmp, &exe).with_context(|| format!("couldn't replace {}", exe.display()))?;

    if let Some(lnk) = start_menu_link(app, o) {
        create_shortcut(&lnk, &exe, &format!("{} {}", app.name, app.version))?;
    }
    write_entries(app, o, &exe, &dir, payload.len())?;
    after(&exe)?;
    // something else (a slow old uninstaller) may have rewritten them meanwhile
    if !entries_ok(app, o, &exe) {
        std::thread::sleep(std::time::Duration::from_secs(1));
        write_entries(app, o, &exe, &dir, payload.len())?;
        anyhow::ensure!(entries_ok(app, o, &exe), "couldn't write the Settings → Apps entry");
    }
    if o.launch || (!o.silent && ask(&title, &format!("{} {} is installed.\n\nStart it now?", display(app, o), app.version))) {
        std::process::Command::new(&exe).current_dir(&dir).spawn().context("couldn't start it")?;
    }
    Ok(exe)
}

fn entries_ok(app: &AppInfo, o: &Opts, exe: &Path) -> bool {
    let hk = RegKey::predef(HKEY_CURRENT_USER);
    let want = exe.display().to_string();
    let un: String = hk.open_subkey(format!(r"{UNINSTALL}\{}", key_name(app, o))).and_then(|k| k.get_value("UninstallString")).unwrap_or_default();
    let ver: String = hk.open_subkey(format!(r"{UNINSTALL}\{}", key_name(app, o))).and_then(|k| k.get_value("DisplayVersion")).unwrap_or_default();
    let run: Option<String> = hk.open_subkey(RUN).and_then(|k| k.get_value(display(app, o))).ok();
    let run_ok = if o.autostart { run.is_some_and(|r| r.contains(&want)) } else { run.is_none() };
    un.contains(&want) && ver == app.version && run_ok
}

fn write_entries(app: &AppInfo, o: &Opts, exe: &Path, dir: &Path, size: usize) -> Result<()> {
    let hk = RegKey::predef(HKEY_CURRENT_USER);
    // a fresh key: no values left over from an older build
    let _ = hk.delete_subkey_all(format!(r"{UNINSTALL}\{}", key_name(app, o)));
    let (k, _) = hk.create_subkey(format!(r"{UNINSTALL}\{}", key_name(app, o)))?;
    let suffix_arg = if o.suffix.is_empty() { String::new() } else { format!(" --suffix \"{}\"", o.suffix) };
    k.set_value("DisplayName", &display(app, o))?;
    k.set_value("DisplayVersion", &app.version)?;
    k.set_value("Publisher", &app.publisher)?;
    k.set_value("DisplayIcon", &format!("\"{}\",0", exe.display()))?;
    k.set_value("InstallLocation", &dir.display().to_string())?;
    k.set_value("UninstallString", &format!("\"{}\" --uninstall{suffix_arg}", exe.display()))?;
    k.set_value("QuietUninstallString", &format!("\"{}\" --uninstall --silent{suffix_arg}", exe.display()))?;
    k.set_value("NoModify", &1u32)?;
    k.set_value("NoRepair", &1u32)?;
    k.set_value("EstimatedSize", &((size / 1024) as u32))?;
    let (run, _) = hk.create_subkey(RUN)?;
    if o.autostart {
        let args = if app.autostart_args.is_empty() { String::new() } else { format!(" {}", app.autostart_args) };
        run.set_value(display(app, o), &format!("\"{}\"{args}", exe.display()))?;
    } else {
        let _ = run.delete_value(display(app, o));
    }
    Ok(())
}

// ---------------------------------------------------------------- uninstall

/// Remove the install this exe belongs to. `extra` removes app-specific entries
/// (Utsuri's right-click menu). Settings and history stay.
pub fn uninstall(app: &AppInfo, o: &Opts, extra: impl FnOnce() -> Result<()>) -> Result<()> {
    let title = format!("Uninstall {}", display(app, o));
    if !o.silent && !ask(&title, &format!("Remove {} from this PC?\n\nYour settings are kept.", display(app, o))) {
        return Ok(());
    }
    let me = std::env::current_exe()?;
    let dir = me.parent().map(Path::to_path_buf).context("no install folder")?;
    stop(&me);
    extra()?;
    let hk = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(run) = hk.open_subkey_with_flags(RUN, winreg::enums::KEY_ALL_ACCESS) {
        // only our own entry
        let v: String = run.get_value(display(app, o)).unwrap_or_default();
        if v.to_lowercase().contains(&me.to_string_lossy().to_lowercase()) {
            let _ = run.delete_value(display(app, o));
        }
    }
    let _ = hk.delete_subkey_all(format!(r"{UNINSTALL}\{}", key_name(app, o)));
    if let Some(lnk) = start_menu_link(app, o) {
        let _ = std::fs::remove_file(lnk);
    }
    // this exe can't delete itself while running: a short-lived cmd removes the folder
    let script = format!("/C ping 127.0.0.1 -n 3 >nul & rmdir /s /q \"{}\"", dir.display());
    std::process::Command::new("cmd").raw_arg(script).creation_flags(NO_WINDOW | 0x0000_0008).spawn()?;
    if !o.silent {
        tell(&title, &format!("{} was removed.", display(app, o)), false);
    }
    Ok(())
}
