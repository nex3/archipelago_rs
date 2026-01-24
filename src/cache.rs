use std::{env, path::PathBuf};

use smol::fs;
use ustr::UstrMap;

use crate::{protocol::GameData, util};

/// The cache in which we store data packages to avoid requesting them every
/// time the client starts.
pub struct Cache(PathBuf);

impl Cache {
    /// Returns a cache that uses Archipelago's system-wide shared directory.
    /// This allows the client to share datapackages with other games, even if
    /// they use other client libraries.
    pub fn shared() -> Self {
        Self(Self::platform_cache_dir().unwrap_or_else(|| {
            env::current_dir()
                .expect("failed to determine current working directory")
                .join("Archipelago")
                .join("Cache")
        }))
    }

    /// Returns a cache that uses a custom filesystem path.
    pub fn path(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    /// Returns the default Archipelago cache directory for the current
    /// operating system.
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

    /// Returns a map from game names to the cached game data for those games.
    ///
    /// `checksums` is a map from the names of games to load to checksums for
    /// each of those games. These checksums indicate the expected versions each
    /// game. If the cache doesn't have a [GameData] for a game with a matching
    /// checksum, that game won't be returned.
    pub(crate) async fn load_data_packages(
        &self,
        checksums: &UstrMap<String>,
    ) -> UstrMap<GameData> {
        let mut data_packages = UstrMap::with_capacity_and_hasher(checksums.len(), Default::default());
        let dir = self.data_package_path();
        for (game, checksum) in checksums {
            let path = dir.join(game).join(format!("{checksum}.json"));

            let file = match fs::read_to_string(&path).await {
                Ok(f) => f,
                Err(err) => {
                    log::error!("Missing or unreadable cache for {}: {}", game, err);
                    continue;
                }
            };

            match serde_json::from_str::<GameData>(&file) {
                // Double-check that the checksum is accurate
                Ok(data) if data.checksum.eq(checksum) => {
                    data_packages.insert(*game, data);
                }
                Ok(_) => {}
                Err(err) => {
                    log::error!(
                        "Failed to deserialize cached data package for {}: {}",
                        game,
                        err
                    );
                }
            }
        }
        data_packages
    }

    /// Stores `data_packages`, a map from game names to data packages, in the
    /// cache.
    pub(crate) async fn store_data_packages(&self, data_packages: &UstrMap<GameData>) {
        let dir = self.data_package_path();
        for (game, data) in data_packages {
            let game_dir = dir.join(game);

            if let Err(err) = fs::create_dir_all(&game_dir).await {
                log::error!("Failed to create cache directory {game_dir:?}: {err}");
                // If one directory fails to create, chances are the others will
                // as well.
                return;
            }

            let serialized = match serde_json::to_string(&data) {
                Ok(r) => r,
                Err(err) => {
                    log::error!("Failed to serialize data package for {game}: {err}");
                    continue;
                }
            };

            let path = game_dir.join(format!("{}.json", data.checksum));
            if let Err(err) = util::write_file_atomic(&path, serialized).await {
                log::error!("Failed to write cached data package to {path:?}: {err}");
            }
        }
    }

    /// Returns the subdirectory that should contain datapackages.
    fn data_package_path(&self) -> PathBuf {
        // We could just use this as the root of the cache, but this is more
        // forward-compatible with the possibility of caching other data in the
        // future.
        self.0.join("datapackage")
    }
}

impl Default for Cache {
    /// Returns [Cache::shared].
    fn default() -> Self {
        Cache::shared()
    }
}
