use anyhow::{Context, Result};
use iroh::SecretKey;
use std::{fs, path::PathBuf};

pub fn config_dir() -> Result<PathBuf> {
    let base = match std::env::var_os("P2P_CHAT_HOME") {
        Some(value) if !value.is_empty() => return prepare(PathBuf::from(value)),
        _ => match std::env::var_os("XDG_CONFIG_HOME") {
            Some(value) if !value.is_empty() => PathBuf::from(value),
            _ => {
                let home =
                    std::env::var_os("HOME").context("neither XDG_CONFIG_HOME nor HOME is set")?;
                PathBuf::from(home).join(".config")
            }
        },
    };

    prepare(base.join("p2p-chat"))
}

fn prepare(dir: PathBuf) -> Result<PathBuf> {
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    Ok(dir)
}

pub fn load_or_create_secret_key() -> Result<SecretKey> {
    let path = config_dir()?.join("identity.key");

    if path.exists() {
        let stored =
            fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
        let bytes: [u8; 32] = stored
            .as_slice()
            .try_into()
            .ok()
            .with_context(|| format!("{} is not a 32 byte key", path.display()))?;
        return Ok(SecretKey::from_bytes(&bytes));
    }

    let bytes: [u8; 32] = rand::random();
    write_private(&path, &bytes)?;
    Ok(SecretKey::from_bytes(&bytes))
}

pub(crate) fn write_private(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}
