use crate::{
    collections::Collection,
    paths::{self, get_share_path},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Repo {
    name: String,
    remote: String,
}

impl Repo {
    pub fn new(name: impl Into<String>, remote: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            remote: remote.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub remote_private: bool,
    pub remote_name: String,
    pub collections: Vec<Repo>,
}

impl Config {
    pub fn config_path() -> PathBuf {
        paths::config_dir().join("config.toml")
    }

    // Save the config to a file
    pub fn save(&self) -> std::io::Result<()> {
        let toml = toml::to_string(&self).expect("Failed to serialize config");
        let mut file = File::create(Self::config_path())?;
        file.write_all(toml.as_bytes())?;
        Ok(())
    }

    // Load the config from a file
    pub fn load() -> std::io::Result<Config> {
        let mut file = match File::open(Self::config_path()) {
            Ok(file) => file,
            Err(_) => {
                let _ =
                    std::fs::rename(Self::config_path(), get_share_path().join("invalid_config"));
                Self::default().save()?;
                File::open(Self::config_path())?
            }
        };

        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let config: Config = toml::from_str(&contents).expect("Failed to deserialize config");
        config.apply();
        Ok(config)
    }

    pub fn apply(&self) {
        for repo in &self.collections {
            let col = Collection::load_or_create(&repo.name);
            col.set_remote(&repo.remote);
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            remote_private: true,
            remote_name: "speki_remote".to_string(),
            collections: vec![Repo::new(
                "https://github.com/TBS1996/spekigraph.git",
                "main",
            )],
        }
    }
}
