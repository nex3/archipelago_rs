use crate::protocol::GameData;
use smol::fs;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::LazyLock;
use ustr::{Ustr, UstrMap};

/// Archipelago's Shared Cache folder
pub static AP_CACHE: LazyLock<PathBuf> = LazyLock::new(|| {
    platform_cache_dir().unwrap_or_else(|| {
        env::current_dir()
            .expect("failed to determine current working directory")
            .join("Archipelago")
            .join("Cache")
    })
});

// This could be merged with the static above, but separate for now in the event we also add support for the common cache
pub static AP_DATA_PACKAGE_CACHE: LazyLock<PathBuf> =
    LazyLock::new(|| AP_CACHE.join("datapackage"));

fn platform_cache_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("Archipelago").join("Cache"))
    }

    #[cfg(target_os = "macos")]
    {
        env::var_os("HOME").map(PathBuf::from).map(|h| {
            h.join("Library")
                .join("Caches")
                .join("Archipelago")
                .join("Cache")
        })
    }

    #[cfg(target_os = "linux")]
    {
        env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| env::home_dir().map(|h| h.join(".cache")))
            .map(|p| p.join("Archipelago").join("Cache"))
    }
}

/// Checks each checksum against the shared archipelago cache to see if it's valid.
/// Returns a map containing each successfully loaded data package
pub(crate) async fn validate_and_load_data_packages(
    checksums: &UstrMap<String>,
    cache_path: &Option<PathBuf>,
) -> HashMap<Ustr, GameData> {
    let mut data_packages = HashMap::new();
    let path = cache_path
        .as_ref()
        .unwrap_or_else(|| AP_DATA_PACKAGE_CACHE.deref());
    for (game_name, checksum) in checksums {
        // Utilize the custom path if specified, otherwise use the default Archipelago Shared Cache
        let dp_file = path.join(game_name).join(format!("{checksum}.json"));

        let file = match fs::read_to_string(&dp_file).await {
            Ok(f) => f,
            Err(err) => {
                log::error!("Missing or unreadable cache for {}: {}", game_name, err);
                continue;
            }
        };

        // Have to specify generics
        match serde_json::from_str::<GameData>(&file) {
            Ok(data) => {
                // Final checksum check
                if data.checksum.eq(checksum) {
                    data_packages.insert(*game_name, data);
                }
            }
            Err(err) => {
                log::error!(
                    "Failed to deserialize cached data package for {}: {}",
                    game_name,
                    err
                );
            }
        }
    }

    data_packages
}

/// Write the acquired data package information to the specified cache
pub(crate) async fn write_to_cache(
    data_package: &HashMap<Ustr, GameData>,
    cache_path: &Option<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let data_package_path = cache_path
        .as_ref()
        .unwrap_or_else(|| AP_DATA_PACKAGE_CACHE.deref());
    for (game_name, data) in data_package {
        let game_path = data_package_path.join(game_name);
        fs::create_dir_all(&game_path).await?;
        fs::write(
            game_path.join(format!("{}.json", data.checksum)),
            serde_json::to_string(&data)?,
        )
        .await?;
    }
    Ok(())
}
