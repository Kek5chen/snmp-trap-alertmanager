use clap::Parser;
use config::Config;
use lazy_static::lazy_static;
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::PathBuf;
use time::Duration;
use time::ext::NumericalDuration;

lazy_static! {
    pub static ref CLI: CLISettings = CLISettings::parse();
}

lazy_static! {
    pub static ref CONFIG: Settings = Config::builder()
        .add_source(config::File::with_name(CLI.config_path()))
        .add_source(config::Environment::default())
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();
}

#[derive(Debug, Parser)]
pub struct CLISettings {
    #[arg(long, short, help = "Path of the configuration file [config]")]
    config: Option<PathBuf>,
    #[arg(long, short, help = "Socket Address of the web frontend [127.0.0.1:7788]")]
    listen: Option<SocketAddr>,
}

impl CLISettings {
    pub fn config_path(&self) -> &str {
        match self.config {
            None => "config",
            Some(ref config) => config.to_str().unwrap(),
        }
    }
}

fn web_listen_default() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 7788))
}

fn announce_sec_default() -> u32 {
    60
}

fn community_label_default() -> String {
    "community".to_string()
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    web_url: String,
    #[serde(default = "web_listen_default")]
    web_listen: SocketAddr,
    db_connection_url: String,
    alertmanager_url: String,
    #[serde(default = "announce_sec_default")]
    alertmanager_announce_sec: u32,
    #[serde(default = "community_label_default")]
    alertmanager_community_label: String,
}

impl Settings {
    pub fn web_url(&self) -> &str {
        &self.web_url
    }

    pub fn web_listen(&self) -> SocketAddr {
        CLI.listen.unwrap_or(self.web_listen)
    }

    pub fn db_url(&self) -> &str {
        &self.db_connection_url
    }

    pub fn alertmanager_url(&self) -> &str {
        &self.alertmanager_url
    }

    pub fn alertmanager_announce_duration(&self) -> Duration {
        (self.alertmanager_announce_sec as i64).seconds()
    }

    pub fn alertmanager_community_label(&self) -> &str {
        &self.alertmanager_community_label
    }
}
