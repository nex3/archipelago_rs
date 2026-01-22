use crate::protocol::GameData;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::{env, fs};
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

// Name pending
#[derive(Debug, Default)]
pub(crate) struct ValidatedDataPackages {
    // Contains the data packages for games that validated successfully
    pub(crate) data_packages: HashMap<Ustr, GameData>,
    // Which games failed to validate
    pub(crate) failed_games: Vec<String>,
}

/// Checks each checksum against the shared archipelago cache to see if it's valid.
/// If it does verify successfully, then add it to the current data package, otherwise mark it as needed to be acquired
pub(crate) fn validate_and_load_data_packages(
    checksums: &UstrMap<String>,
    cache_path: &Option<PathBuf>,
) -> ValidatedDataPackages {
    let mut wip_data_package = ValidatedDataPackages::default();
    let path = cache_path
        .as_ref()
        .unwrap_or_else(|| AP_DATA_PACKAGE_CACHE.deref());
    for (game_name, checksum) in checksums {
        // Utilize the custom path if specified, otherwise use the default Archipelago Shared Cache
        let dp_file = path.join(game_name).join(format!("{checksum}.json"));

        let file = match File::open(&dp_file) {
            Ok(f) => f,
            Err(err) => {
                log::error!("Missing or unreadable cache for {}: {}", game_name, err);
                wip_data_package.failed_games.push(game_name.to_string());
                continue;
            }
        };

        // Have to specify generics
        match serde_json::from_reader::<BufReader<File>, GameData>(BufReader::new(file)) {
            Ok(data) => {
                // Final checksum check
                if data.checksum.eq(checksum) {
                    wip_data_package.data_packages.insert(*game_name, data);
                } else {
                    wip_data_package.failed_games.push(game_name.to_string());
                }
            }
            Err(err) => {
                log::error!(
                    "Failed to deserialize cached data package for {}: {}",
                    game_name,
                    err
                );
                wip_data_package.failed_games.push(game_name.to_string());
            }
        }
    }

    wip_data_package
}

/// Write the acquired data package information to the specified cache
pub(crate) fn write_to_cache(
    data_package: &HashMap<Ustr, GameData>,
    cache_path: &Option<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let data_package_path = cache_path
        .as_ref()
        .unwrap_or_else(|| AP_DATA_PACKAGE_CACHE.deref());
    for (game_name, data) in data_package {
        let game_path = data_package_path.join(game_name);
        if !game_path.try_exists()? {
            fs::create_dir_all(&game_path)?;
        }
        fs::write(
            game_path.join(format!("{}.json", data.checksum)),
            serde_json::to_string(&data)?.as_bytes(),
        )?;
    }
    Ok(())
}
