use std::path::PathBuf;

const CONFIG_DIR_NAME: &str = "ade";
const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub project_dir: PathBuf,
    pub opencode_binary: String,
    pub poll_interval_ms: u64,
    pub busy_poll_interval_ms: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            project_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            opencode_binary: "opencode".to_string(),
            poll_interval_ms: 2_000,
            busy_poll_interval_ms: 600,
        }
    }
}

impl AppConfig {
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME))
    }

    pub fn load() -> Self {
        let mut cfg = Self::default();
        let Some(path) = Self::config_path() else {
            return cfg;
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return cfg;
        };
        let Ok(value) = raw.parse::<toml::Table>() else {
            return cfg;
        };
        if let Some(s) = value.get("project_dir").and_then(|v| v.as_str()) {
            cfg.project_dir = PathBuf::from(expand_home(s));
        }
        if let Some(s) = value.get("opencode_binary").and_then(|v| v.as_str()) {
            cfg.opencode_binary = s.to_string();
        }
        if let Some(n) = value.get("poll_interval_ms").and_then(|v| v.as_integer()) {
            cfg.poll_interval_ms = (n.max(200)) as u64;
        }
        if let Some(n) = value
            .get("busy_poll_interval_ms")
            .and_then(|v| v.as_integer())
        {
            cfg.busy_poll_interval_ms = (n.max(150)) as u64;
        }
        cfg
    }
}

fn expand_home(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest).to_string_lossy().into_owned();
    }
    s.to_string()
}
