//! Project location policy for novice users.
//!
//! A user creating a project only provides the 作品名 (name). The app
//! computes the on-disk location automatically:
//!
//! ```text
//! {Documents}/LingBi/<sanitized-name>/
//! ```
//!
//! with a `-2`, `-3`, ... suffix when the folder already exists, so a
//! name-only flow never fails on a collision and never touches existing
//! files. The name is sanitized so it is a valid folder name on Windows
//! (the P0 platform) while keeping Chinese characters intact.

use lingbi_contracts::{AppError, ErrorCode};
use std::fs;
use std::path::{Path, PathBuf};

/// Characters that are illegal in Windows file/folder names.
const ILLEGAL: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
/// Windows reserved device names; a folder with one of these names is
/// rejected or shadowed on Windows.
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];
const FALLBACK_NAME: &str = "未命名作品";
const MAX_LEN: usize = 60;

/// Sanitize a project name into a valid, human-readable folder name.
pub fn sanitize_project_name(name: &str) -> String {
    let mut sanitized: String = name
        .chars()
        .map(|character| {
            if ILLEGAL.contains(&character) || character.is_control() {
                '_'
            } else {
                character
            }
        })
        .collect();
    sanitized = sanitized.trim().trim_end_matches(['.', ' ']).to_owned();
    if sanitized.is_empty() {
        return FALLBACK_NAME.to_owned();
    }
    let stem = sanitized.split('.').next().unwrap_or(&sanitized);
    if RESERVED
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(stem))
    {
        sanitized.push('_');
    }
    if sanitized.chars().count() > MAX_LEN {
        sanitized = sanitized.chars().take(MAX_LEN).collect();
    }
    sanitized
}

/// The base directory holding all novice projects: `{Documents}/LingBi`.
/// Falls back to `{home}/LingBi` when the Documents folder cannot be
/// resolved (e.g. minimal CI containers).
pub fn default_projects_root() -> Result<PathBuf, AppError> {
    if let Some(documents) = dirs::document_dir() {
        return Ok(documents.join("LingBi"));
    }
    if let Some(home) = dirs::home_dir() {
        return Ok(home.join("LingBi"));
    }
    Err(AppError::new(
        ErrorCode::ProjectCorrupted,
        "cannot resolve Documents or home directory".to_owned(),
        false,
    ))
}

/// The default root for a named project: `{Documents}/LingBi/<name>`,
/// made unique with a `-N` suffix when the folder already exists.
pub fn default_root_for(name: &str) -> Result<PathBuf, AppError> {
    unique_root(&default_projects_root()?, &sanitize_project_name(name))
}

/// First free `<base>/<name>[-N]` path that does not exist yet.
pub fn unique_root(base: &Path, name: &str) -> Result<PathBuf, AppError> {
    let candidate = base.join(name);
    if !candidate.exists() {
        return Ok(candidate);
    }
    for index in 2..10_000u32 {
        let next = base.join(format!("{name}-{index}"));
        if !next.exists() {
            return Ok(next);
        }
    }
    Err(AppError::new(
        ErrorCode::ProjectPathExists,
        format!("no free project folder for {name} under {}", base.display()),
        false,
    ))
}

/// Ensure the folder exists (used by create flows that already computed a
/// unique root).
pub fn ensure_root(root: &Path) -> Result<(), AppError> {
    if root.exists() {
        return Err(AppError::new(
            ErrorCode::ProjectPathExists,
            format!("project path already exists: {}", root.display()),
            false,
        ));
    }
    fs::create_dir_all(root).map_err(|error| {
        AppError::new(
            ErrorCode::ProjectCorrupted,
            format!("failed to create project folder: {error}"),
            false,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn sanitize_keeps_chinese_and_removes_illegal_windows_chars() {
        assert_eq!(sanitize_project_name("我的小说:雨夜"), "我的小说_雨夜");
        assert_eq!(
            sanitize_project_name("a<b>c:d\"e/f\\g|h?i*j"),
            "a_b_c_d_e_f_g_h_i_j"
        );
    }

    #[test]
    fn sanitize_trims_and_trims_trailing_dots_spaces() {
        assert_eq!(sanitize_project_name("  雨夜.  "), "雨夜");
        assert_eq!(sanitize_project_name("雨夜..."), "雨夜");
    }

    #[test]
    fn sanitize_handles_windows_reserved_device_names() {
        assert_eq!(sanitize_project_name("CON"), "CON_");
        assert_eq!(sanitize_project_name("com1"), "com1_");
        assert_eq!(sanitize_project_name("NUL"), "NUL_");
        assert_eq!(sanitize_project_name("普通小说"), "普通小说");
    }

    #[test]
    fn sanitize_empty_and_whitespace_fall_back() {
        assert_eq!(sanitize_project_name(""), "未命名作品");
        assert_eq!(sanitize_project_name("   "), "未命名作品");
    }

    #[test]
    fn sanitize_handles_control_characters() {
        assert_eq!(sanitize_project_name("a\u{0}b\u{1}c"), "a_b_c");
    }

    #[test]
    fn default_root_lives_under_documents_lingbi() {
        let root = default_root_for("我的小说").expect("default root");
        let mut parts = root.iter().rev();
        assert_eq!(
            parts.next().and_then(|part| part.to_str()),
            Some("我的小说")
        );
        assert_eq!(parts.next().and_then(|part| part.to_str()), Some("LingBi"));
        assert!(root.is_absolute());
    }

    #[test]
    fn unique_root_appends_suffix_on_collision() {
        let temp = TempDir::new().expect("temp dir");
        let first = unique_root(temp.path(), "小说").expect("first");
        fs::create_dir_all(&first).expect("create first");
        let second = unique_root(temp.path(), "小说").expect("second");
        assert_eq!(second, temp.path().join("小说-2"));
        fs::create_dir_all(&second).expect("create second");
        let third = unique_root(temp.path(), "小说").expect("third");
        assert_eq!(third, temp.path().join("小说-3"));
    }

    #[test]
    fn ensure_root_refuses_existing_path() {
        let temp = TempDir::new().expect("temp dir");
        let existing = temp.path().join("exists");
        fs::create_dir_all(&existing).expect("create");
        let result = ensure_root(&existing);
        assert!(matches!(
            result,
            Err(AppError {
                code: ErrorCode::ProjectPathExists,
                ..
            })
        ));
    }
}
