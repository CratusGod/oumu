//! "Start Oumu with Windows": the user's Run key (HKCU\...\CurrentVersion\Run),
//! the place Windows' Startup apps settings list. The key itself is the setting,
//! so turning it off in Task Manager or Settings shows here too.

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE,
    KEY_SET_VALUE, REG_SAM_FLAGS, REG_SZ,
};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE: &str = "Oumu";

fn open(access: REG_SAM_FLAGS) -> Option<HKEY> {
    let mut key = HKEY::default();
    (unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, access, &mut key) } == ERROR_SUCCESS).then_some(key)
}

/// The command Windows runs at sign-in: this exe, quoted.
fn command() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(format!("\"{}\"", exe.display()))
}

/// Oumu is in the Run key.
pub fn enabled() -> bool {
    let Some(key) = open(KEY_QUERY_VALUE) else { return false };
    let found = unsafe { RegQueryValueExW(key, &HSTRING::from(VALUE), None, None, None, None) } == ERROR_SUCCESS;
    unsafe {
        let _ = RegCloseKey(key);
    }
    found
}

/// Add or remove Oumu from the Run key; returns the state after.
pub fn set(on: bool) -> Result<bool, String> {
    let key = open(KEY_SET_VALUE).ok_or("the Run key can't be opened")?;
    let name = HSTRING::from(VALUE);
    let res = if on {
        let mut data: Vec<u16> = command()?.encode_utf16().collect();
        data.push(0);
        let bytes = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2) };
        unsafe { RegSetValueExW(key, &name, None, REG_SZ, Some(bytes)) }
    } else {
        match unsafe { RegDeleteValueW(key, &name) } {
            e if e == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND => ERROR_SUCCESS,
            e => e,
        }
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    if res != ERROR_SUCCESS {
        return Err(format!("Windows refused to change the Run key ({})", res.0));
    }
    Ok(enabled())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the user's real Run key and puts it back as it was.
    #[test]
    fn turns_on_and_off_and_restores_the_users_choice() {
        let before = enabled();
        assert!(set(true).unwrap());
        assert!(enabled());
        assert!(!set(false).unwrap());
        assert!(!set(false).unwrap(), "turning it off twice is fine");
        set(before).unwrap();
        assert_eq!(enabled(), before);
    }

    #[test]
    fn the_command_quotes_the_exe() {
        let c = command().unwrap();
        assert!(c.starts_with('"') && c.ends_with('"') && c.to_lowercase().contains(".exe"));
    }
}
