//! Recent projects persistence for the desktop shell.
//!
//! Novice users reopen their novel from the first-launch screen ("打开最近
//! 项目") without understanding file paths. The list lives in the app data
//! directory as a small JSON file; it is best-effort: if the file cannot be
//! read the app starts with an empty list instead of failing.

use chrono::{DateTime, Utc};
use lingbi_contracts::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const RECENT_LIMIT: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentProject {
    pub name: String,
    pub root: String,
    pub last_opened: DateTime<Utc>,
}

pub struct RecentProjects {
    path: PathBuf,
}

impl RecentProjects {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<Vec<RecentProject>, AppError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let bytes = fs::read(&self.path).map_err(io_error)?;
        let projects: Vec<RecentProject> =
            serde_json::from_slice(&bytes).map_err(parse_error)?;
        Ok(projects)
    }

    /// Upsert by root (move to front), cap the list, persist atomically.
    pub fn record(&self, name: &str, root: &str) -> Result<Vec<RecentProject>, AppError> {
        let mut projects = self.load()?;
        projects.retain(|project| project.root != root);
        projects.insert(
            0,
            RecentProject {
                name: name.to_owned(),
                root: root.to_owned(),
                last_opened: Utc::now(),
            },
        );
        projects.truncate(RECENT_LIMIT);
        self.save(&projects)?;
        Ok(projects)
    }

    fn save(&self, projects: &[RecentProject]) -> Result<(), AppError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        let bytes = serde_json::to_vec(projects).map_err(parse_error)?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, &bytes).map_err(io_error)?;
        fs::rename(&tmp, &self.path).map_err(io_error)?;
        Ok(())
    }
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("recent projects I/O failed: {error}"),
        false,
    )
}

fn parse_error(error: serde_json::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("recent projects parse failed: {error}"),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_missing_file_returns_empty() {
        let temp = TempDir::new().expect("temp dir");
        let recent = RecentProjects::new(temp.path().join("recent.json"));
        assert!(recent.load().expect("load").is_empty());
    }

    #[test]
    fn record_upserts_by_root_and_orders_most_recent_first() {
        let temp = TempDir::new().expect("temp dir");
        let recent = RecentProjects::new(temp.path().join("recent.json"));

        let _ = recent.record("小说一", "/p/one").expect("one");
        let _ = recent.record("小说二", "/p/two").expect("two");
        let projects = recent.record("小说一", "/p/one").expect("one again");

        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "小说一");
        assert_eq!(projects[0].root, "/p/one");
        assert_eq!(projects[1].root, "/p/two");
    }

    #[test]
    fn record_caps_at_limit() {
        let temp = TempDir::new().expect("temp dir");
        let recent = RecentProjects::new(temp.path().join("recent.json"));
        for index in 0..(RECENT_LIMIT + 5) {
            recent
                .record(&format!("小说{index}"), &format!("/p/{index}"))
                .expect("record");
        }
        let projects = recent.load().expect("load");
        assert_eq!(projects.len(), RECENT_LIMIT);
        assert_eq!(projects[0].root, format!("/p/{}", RECENT_LIMIT + 4));
    }

    #[test]
    fn record_survives_reload() {
        let temp = TempDir::new().expect("temp dir");
        let path = temp.path().join("recent.json");
        RecentProjects::new(path.clone())
            .record("我的小说", "/p/novel")
            .expect("record");

        let projects = RecentProjects::new(path).load().expect("load");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "我的小说");
        assert_eq!(projects[0].root, "/p/novel");
    }
}
