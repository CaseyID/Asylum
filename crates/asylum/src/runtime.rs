use std::env;
use std::path::{Path, PathBuf};

use anyhow::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimePaths {
    pub home: PathBuf,
    pub config: PathBuf,
    pub database: PathBuf,
    pub log: PathBuf,
    pub pid: PathBuf,
}

impl RuntimePaths {
    pub fn from_env(config_override: Option<PathBuf>) -> Result<Self> {
        Ok(Self::from_values(
            env::var_os("ASYLUM_HOME").map(PathBuf::from),
            config_override.or_else(|| env::var_os("ASYLUM_CONFIG").map(PathBuf::from)),
            env::var_os("ASYLUM_DATABASE").map(PathBuf::from),
            env::var_os("HOME").map(PathBuf::from),
        ))
    }

    pub fn from_values(
        asylum_home: Option<PathBuf>,
        config_override: Option<PathBuf>,
        database_override: Option<PathBuf>,
        user_home: Option<PathBuf>,
    ) -> Self {
        let home = asylum_home.unwrap_or_else(|| {
            user_home
                .unwrap_or_else(|| Path::new(".").to_path_buf())
                .join(".asylum")
        });
        let config = config_override.unwrap_or_else(|| home.join("config.toml"));
        let database = database_override.unwrap_or_else(|| home.join("asylum.sqlite3"));
        Self {
            log: home.join("logs").join("asylum.log"),
            pid: home.join("run").join("asylum.pid"),
            home,
            config,
            database,
        }
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.home)?;
        std::fs::create_dir_all(self.logs_dir())?;
        std::fs::create_dir_all(self.run_dir())?;
        if let Some(parent) = self.config.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if let Some(parent) = self.database.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.home.join("logs")
    }

    pub fn run_dir(&self) -> PathBuf {
        self.home.join("run")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_paths_live_under_user_asylum_home() {
        let paths =
            RuntimePaths::from_values(None, None, None, Some(PathBuf::from("/Users/example")));
        assert_eq!(paths.home, PathBuf::from("/Users/example/.asylum"));
        assert_eq!(
            paths.config,
            PathBuf::from("/Users/example/.asylum/config.toml")
        );
        assert_eq!(
            paths.database,
            PathBuf::from("/Users/example/.asylum/asylum.sqlite3")
        );
        assert_eq!(
            paths.log,
            PathBuf::from("/Users/example/.asylum/logs/asylum.log")
        );
        assert_eq!(
            paths.pid,
            PathBuf::from("/Users/example/.asylum/run/asylum.pid")
        );
    }

    #[test]
    fn asylum_home_controls_product_paths() {
        let paths =
            RuntimePaths::from_values(Some(PathBuf::from("/tmp/asylum-home")), None, None, None);
        assert_eq!(paths.config, PathBuf::from("/tmp/asylum-home/config.toml"));
        assert_eq!(
            paths.database,
            PathBuf::from("/tmp/asylum-home/asylum.sqlite3")
        );
    }

    #[test]
    fn explicit_config_and_database_override_product_defaults() {
        let paths = RuntimePaths::from_values(
            Some(PathBuf::from("/tmp/asylum-home")),
            Some(PathBuf::from("/tmp/config.toml")),
            Some(PathBuf::from("/tmp/db.sqlite3")),
            None,
        );
        assert_eq!(paths.config, PathBuf::from("/tmp/config.toml"));
        assert_eq!(paths.database, PathBuf::from("/tmp/db.sqlite3"));
        assert_eq!(paths.log, PathBuf::from("/tmp/asylum-home/logs/asylum.log"));
    }
}
