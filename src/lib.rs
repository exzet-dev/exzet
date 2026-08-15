pub mod cas;
pub mod cell;
pub mod exfile;
pub mod nfs;
pub mod proto;

pub const DEFAULT_PORT: u16 = 7433;

pub fn config_dir() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    return std::env::var_os("APPDATA").map(std::path::PathBuf::from);
    #[cfg(unix)]
    std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
}

pub fn rand_hex(n: usize) -> String {
    let mut buf = vec![0u8; n];
    getrandom::fill(&mut buf).expect("entropy");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn is_root() -> bool {
    #[cfg(unix)]
    return rustix::process::geteuid().is_root();
    #[cfg(windows)]
    true
}

pub fn set_mode(path: &std::path::Path, mode: u32) -> std::io::Result<()> {
    #[cfg(unix)]
    return std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(mode));
    #[cfg(windows)]
    {
        let _ = (path, mode);
        Ok(())
    }
}

pub fn mode_of(meta: &std::fs::Metadata) -> u32 {
    #[cfg(unix)]
    return std::os::unix::fs::PermissionsExt::mode(&meta.permissions()) & 0o777;
    #[cfg(windows)]
    {
        let _ = meta;
        0o755
    }
}
