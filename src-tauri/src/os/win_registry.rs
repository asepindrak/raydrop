#[cfg(target_os = "windows")]
pub fn register_windows_context_menu() -> Result<String, String> {
    use std::env;
    use winreg::enums::*;
    use winreg::RegKey;

    let exe_path = env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe_path.to_string_lossy().to_string();
    let command_str = format!("\"{}\" --send \"%1\"", exe_str);
    let icon_str = format!("{},0", exe_str);

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

    Ok("Windows Explorer context menus (Files, Folders & SendTo) registered successfully.".to_string())
}

#[cfg(not(target_os = "windows"))]
pub fn register_windows_context_menu() -> Result<String, String> {
    Err("Windows Registry integration is only available on Windows.".to_string())
}

