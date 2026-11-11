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

