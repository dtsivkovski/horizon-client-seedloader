//! Seeds per server and dimension, in `config/client_seed_loader.toml`.
//! Small TOML subset, parsed by hand to avoid dependencies.
//!
//! ```toml
//! radius = 0
//!
//! [servers."play.example.net"]
//! seed = 123
//! "minecraft:the_nether" = 456
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

pub const PATH: &str = "config/client_seed_loader.toml";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Server {
    /// World seed for every dimension.
    pub seed: Option<i64>,
    /// Per-dimension seeds. These override `seed`.
    pub dimensions: BTreeMap<String, i64>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Config {
    /// Generation radius in chunks. 0 follows far-terrain distance.
    pub radius: i32,
    pub servers: BTreeMap<String, Server>,
}

impl Config {
    /// Seed for `level` on `server`. Dimension seed wins over server seed.
    pub fn seed(&self, server: &str, level: &str) -> Option<i64> {
        let server = self.servers.get(server)?;
        server.dimensions.get(level).copied().or(server.seed)
    }

    /// Reads config file, or defaults if missing.
    pub fn load() -> Self {
        std::fs::read_to_string(path())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Writes config file, creating its folder if needed.
    pub fn save(&self) -> std::io::Result<()> {
        let path = path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_text())
    }

    /// Skips lines it doesn't understand.
    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        let mut server: Option<String> = None;
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line
                .strip_prefix("[servers.")
                .and_then(|rest| rest.strip_suffix(']'))
            {
                let name = unquote(name).to_owned();
                config.servers.entry(name.clone()).or_default();
                server = Some(name);
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (unquote(key.trim()), value.trim());
            match &server {
                None if key == "radius" => config.radius = value.parse().unwrap_or(0),
                Some(name) => {
                    let Ok(seed) = value.parse() else { continue };
                    let entry = config.servers.entry(name.clone()).or_default();
                    if key == "seed" {
                        entry.seed = Some(seed);
                    } else {
                        entry.dimensions.insert(key.to_owned(), seed);
                    }
                }
                None => {}
            }
        }
        config
    }

    /// Config as TOML text.
    pub fn to_text(&self) -> String {
        let mut text = String::from(
            "# Client Seed Loader: world seeds per server. Edit with /lodseed in game.\n",
        );
        text += &format!("radius = {}\n", self.radius);
        for (name, server) in &self.servers {
            text += &format!("\n[servers.\"{name}\"]\n");
            if let Some(seed) = server.seed {
                text += &format!("seed = {seed}\n");
            }
            for (dimension, seed) in &server.dimensions {
                text += &format!("\"{dimension}\" = {seed}\n");
            }
        }
        text
    }
}

/// Relative to game folder (game's working directory).
fn path() -> PathBuf {
    PathBuf::from(PATH)
}

/// Strips surrounding quotes, if any.
fn unquote(s: &str) -> &str {
    s.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Config survives a save and load unchanged.
    #[test]
    fn round_trips() {
        let mut config = Config {
            radius: 64,
            ..Config::default()
        };
        let server = config.servers.entry("play.example.net".into()).or_default();
        server.seed = Some(-42);
        server.dimensions.insert("minecraft:the_nether".into(), 7);
        assert_eq!(Config::parse(&config.to_text()), config);
    }

    /// Dimension seed overrides server seed.
    #[test]
    fn dimension_seeds_win() {
        let config = Config::parse(
            "[servers.\"a\"]\nseed = 1\n\"minecraft:the_end\" = 2\n[servers.\"b\"]\n",
        );
        assert_eq!(config.seed("a", "minecraft:overworld"), Some(1));
        assert_eq!(config.seed("a", "minecraft:the_end"), Some(2));
        assert_eq!(config.seed("b", "minecraft:overworld"), None);
        assert_eq!(config.seed("c", "minecraft:overworld"), None);
    }
}
