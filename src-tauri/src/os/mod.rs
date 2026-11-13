pub mod linux_nautilus;
pub mod mac_finder;
pub mod win_registry;

pub fn register_context_menu() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        return win_registry::register_windows_context_menu();
    }
    #[cfg(target_os = "linux")]
    {
        return linux_nautilus::register_nautilus_script();
    }
    #[cfg(target_os = "macos")]
    {
        return mac_finder::register_macos_finder_service();
    }
    #[allow(unreachable_code)]
    Err("Unsupported operating system for context menu integration".to_string())
}

pub fn set_autostart(enabled: bool) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        return win_registry::set_windows_autostart(enabled);
    }
    #[cfg(target_os = "linux")]
    {
        return linux_nautilus::set_linux_autostart(enabled);
    }
    #[cfg(target_os = "macos")]
    {
        return mac_finder::set_macos_autostart(enabled);
    }
    #[allow(unreachable_code)]
    Err("Unsupported operating system for autostart".to_string())
}

