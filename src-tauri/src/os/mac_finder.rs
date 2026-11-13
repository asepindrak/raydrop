pub fn register_macos_finder_service() -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let services_dir = std::path::PathBuf::from(home)
        .join("Library")
        .join("Services")
        .join("Send with Raydrop.workflow")
        .join("Contents");
    std::fs::create_dir_all(&services_dir).map_err(|e| e.to_string())?;

    let exe_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "/Applications/Raydrop.app/Contents/MacOS/raydrop".to_string());

    let info_plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>NSServices</key>
    <array>
        <dict>
            <key>NSMenuItem</key>
            <dict>
                <key>default</key>
                <string>Send with Raydrop</string>
            </dict>
            <key>NSMessage</key>
            <string>runWorkflowAsService</string>
            <key>NSRequiredContext</key>
            <dict>
                <key>NSApplicationIdentifier</key>
                <string>com.apple.finder</string>
            </dict>
            <key>NSSendFileTypes</key>
            <array>
                <string>public.item</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
<!-- Executable target: {exe} -->
"#,
        exe = exe_path
    );

    let plist_path = services_dir.join("Info.plist");
    std::fs::write(&plist_path, info_plist).map_err(|e| e.to_string())?;

    Ok(format!(
        "macOS Finder Quick Action registered at {}",
        services_dir.display()
    ))
}

pub fn set_macos_autostart(enabled: bool) -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|e| e.to_string())?;
    let agents_dir = std::path::PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents");
    std::fs::create_dir_all(&agents_dir).map_err(|e| e.to_string())?;
    let plist_file = agents_dir.join("com.raydrop.p2p.plist");

    if enabled {
        let exe_path = std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "/Applications/Raydrop.app/Contents/MacOS/raydrop".to_string());
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.raydrop.p2p</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>--minimized</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#,
            exe = exe_path
        );
        std::fs::write(&plist_file, plist).map_err(|e| e.to_string())?;
        Ok("macOS LaunchAgent Autostart enabled.".to_string())
    } else {
        let _ = std::fs::remove_file(&plist_file);
        Ok("macOS LaunchAgent Autostart disabled.".to_string())
    }
}

