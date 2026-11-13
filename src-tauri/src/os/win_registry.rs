#[cfg(target_os = "windows")]
pub fn register_windows_context_menu() -> Result<String, String> {
    use directories::ProjectDirs;
    use std::env;
    use winreg::enums::*;
    use winreg::RegKey;

    let exe_path = env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe_path.to_string_lossy().to_string();
    let command_str = format!("\"{}\" --send \"%1\"", exe_str);

    // Extract embedded icon.ico to persistent app data directory so Windows Explorer
    // always displays the crisp icon in both dev and production builds.
    let icon_str = if let Some(proj_dirs) = ProjectDirs::from("com", "raydrop", "Raydrop") {
        let data_dir = proj_dirs.data_dir();
        let _ = std::fs::create_dir_all(data_dir);
        let ico_path = data_dir.join("raydrop.ico");
        let ico_bytes = include_bytes!("../../icons/icon.ico");
        let _ = std::fs::write(&ico_path, ico_bytes);
        ico_path.to_string_lossy().to_string()
    } else {
        format!("\"{}\",0", exe_str)
    };

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // 1. Register for all files: HKCU\Software\Classes\*\shell\RaydropSend
    let (file_key, _) = hkcu
        .create_subkey(r"Software\Classes\*\shell\RaydropSend")
        .map_err(|e| e.to_string())?;
    file_key
        .set_value("", &"Send with Raydrop")
        .map_err(|e| e.to_string())?;
    file_key
        .set_value("Icon", &icon_str)
        .map_err(|e| e.to_string())?;

    let (file_cmd_key, _) = file_key
        .create_subkey("command")
        .map_err(|e| e.to_string())?;
    file_cmd_key
        .set_value("", &command_str)
        .map_err(|e| e.to_string())?;

    // 2. Register for folders/directories: HKCU\Software\Classes\Directory\shell\RaydropSend
    let (dir_key, _) = hkcu
        .create_subkey(r"Software\Classes\Directory\shell\RaydropSend")
        .map_err(|e| e.to_string())?;
    dir_key
        .set_value("", &"Send Folder with Raydrop")
        .map_err(|e| e.to_string())?;
    dir_key
        .set_value("Icon", &icon_str)
        .map_err(|e| e.to_string())?;

    let (dir_cmd_key, _) = dir_key
        .create_subkey("command")
        .map_err(|e| e.to_string())?;
    dir_cmd_key
        .set_value("", &command_str)
        .map_err(|e| e.to_string())?;

    // 3. Register multi-file batch selection helper in %APPDATA%\Microsoft\Windows\SendTo
    if let Ok(appdata) = env::var("APPDATA") {
        let sendto_dir = std::path::PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("SendTo");
        if sendto_dir.exists() {
            let cmd_file = sendto_dir.join("Raydrop.cmd");
            let script = format!("@echo off\r\nstart \"\" \"{}\" --send %*\r\n", exe_str);
            let _ = std::fs::write(cmd_file, script);
        }
    }

    Ok("Windows Explorer context menus (Files, Folders & SendTo) registered with custom icon.".to_string())
}

#[cfg(not(target_os = "windows"))]
pub fn register_windows_context_menu() -> Result<String, String> {
    Err("Windows Registry integration is only available on Windows.".to_string())
}

#[cfg(target_os = "windows")]
pub fn set_windows_autostart(enabled: bool) -> Result<String, String> {
    use std::env;
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run_key, _) = hkcu
        .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")
        .map_err(|e| e.to_string())?;

    if enabled {
        let exe_path = env::current_exe().map_err(|e| e.to_string())?;
        let exe_str = exe_path.to_string_lossy().to_string();
        let startup_cmd = format!("\"{}\" --minimized", exe_str);
        run_key
            .set_value("Raydrop", &startup_cmd)
            .map_err(|e| e.to_string())?;
        Ok("Windows Autorun at Startup enabled (starts minimized in System Tray).".to_string())
    } else {
        let _ = run_key.delete_value("Raydrop");
        Ok("Windows Autorun at Startup disabled.".to_string())
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_windows_autostart(_enabled: bool) -> Result<String, String> {
    Err("Windows Registry autostart is only available on Windows.".to_string())
}

#[cfg(target_os = "windows")]
pub fn is_win11_classic_menu_enabled() -> bool {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32")
        .is_ok()
}

#[cfg(target_os = "windows")]
pub fn set_win11_classic_menu(enabled: bool) -> Result<bool, String> {
    use std::os::windows::process::CommandExt;
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let clsid_root = r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}";

    if enabled {
        let (inproc, _) = hkcu
            .create_subkey(&format!(r"{}\InprocServer32", clsid_root))
            .map_err(|e| e.to_string())?;
        inproc.set_value("", &"").map_err(|e| e.to_string())?;
    } else {
        let _ = hkcu.delete_subkey_all(clsid_root);
    }

    // Refresh Windows File Explorer seamlessly without command prompt flicker
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let _ = std::process::Command::new("cmd.exe")
        .args(["/c", "taskkill /f /im explorer.exe & start explorer.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();

    Ok(enabled)
}

#[cfg(not(target_os = "windows"))]
pub fn is_win11_classic_menu_enabled() -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
pub fn set_win11_classic_menu(_enabled: bool) -> Result<bool, String> {
    Ok(false)
}

