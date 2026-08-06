use lingbi_contracts::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

pub const PACKAGE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageManifest {
    pub schema_version: u32,
    pub files: Vec<PackageFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageReceipt {
    pub file_count: usize,
    pub schema_version: u32,
}

pub struct PortablePackageService;

impl PortablePackageService {
    pub fn export_package(
        &self,
        project_root: &Path,
        zip_path: &Path,
    ) -> Result<PackageReceipt, AppError> {
        let mut files = Vec::new();
        collect_files(project_root, project_root, &mut files)?;
        files.sort();

        let mut manifest_files = Vec::new();
        let mut payloads = Vec::new();
        for relative in &files {
            let bytes = fs::read(project_root.join(relative)).map_err(io_error)?;
            let hash = hex_sha256(&bytes);
            manifest_files.push(PackageFile {
                path: relative.clone(),
                sha256: hash,
            });
            payloads.push((relative.clone(), bytes));
        }
        let manifest = PackageManifest {
            schema_version: PACKAGE_SCHEMA_VERSION,
            files: manifest_files,
        };

        let output = File::create(zip_path).map_err(io_error)?;
        let mut zip = zip::ZipWriter::new(output);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let manifest_bytes = serde_json::to_vec(&manifest).map_err(parse_error)?;
        zip.start_file("MANIFEST.json", options)
            .map_err(zip_error)?;
        zip.write_all(&manifest_bytes).map_err(io_error)?;
        for (relative, bytes) in payloads {
            zip.start_file(&relative, options).map_err(zip_error)?;
            zip.write_all(&bytes).map_err(io_error)?;
        }
        zip.finish().map_err(zip_error)?;

        Ok(PackageReceipt {
            file_count: files.len(),
            schema_version: PACKAGE_SCHEMA_VERSION,
        })
    }

    pub fn import_package(
        &self,
        zip_path: &Path,
        destination: &Path,
    ) -> Result<PackageReceipt, AppError> {
        if destination.exists() {
            return Err(AppError::new(
                ErrorCode::ProjectPathExists,
                format!(
                    "package destination already exists: {}",
                    destination.display()
                ),
                false,
            ));
        }

        let file = File::open(zip_path).map_err(io_error)?;
        let mut archive = zip::ZipArchive::new(file).map_err(zip_error)?;
        let mut manifest: Option<PackageManifest> = None;
        let mut entries: Vec<(String, Vec<u8>)> = Vec::new();

        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(zip_error)?;
            let name = entry.name().to_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(io_error)?;
            if name == "MANIFEST.json" {
                manifest = Some(serde_json::from_slice(&bytes).map_err(parse_error)?);
                continue;
            }
            validate_relative_path(&name)?;
            entries.push((name, bytes));
        }

        let manifest = manifest.ok_or_else(|| {
            AppError::new(
                ErrorCode::ProjectCorrupted,
                "portable package has no MANIFEST.json".to_owned(),
                false,
            )
        })?;
        if manifest.schema_version != PACKAGE_SCHEMA_VERSION {
            return Err(AppError::new(
                ErrorCode::PackageChecksumMismatch,
                format!("unsupported package schema: {}", manifest.schema_version),
                false,
            ));
        }

        for package_file in &manifest.files {
            validate_relative_path(&package_file.path)?;
            let payload = entries
                .iter()
                .find(|(path, _)| path == &package_file.path)
                .map(|(_, bytes)| bytes)
                .ok_or_else(|| {
                    AppError::new(
                        ErrorCode::PackageChecksumMismatch,
                        format!("package file missing: {}", package_file.path),
                        false,
                    )
                })?;
            if hex_sha256(payload) != package_file.sha256 {
                return Err(AppError::new(
                    ErrorCode::PackageChecksumMismatch,
                    format!("package checksum mismatch: {}", package_file.path),
                    false,
                ));
            }
        }

        fs::create_dir_all(destination).map_err(io_error)?;
        for (relative, bytes) in entries {
            let target = destination.join(&relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(io_error)?;
            }
            fs::write(target, bytes).map_err(io_error)?;
        }

        Ok(PackageReceipt {
            file_count: manifest.files.len(),
            schema_version: manifest.schema_version,
        })
    }
}

fn collect_files(root: &Path, current: &Path, files: &mut Vec<String>) -> Result<(), AppError> {
    for entry in fs::read_dir(current).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| {
                    AppError::new(
                        ErrorCode::ProjectCorrupted,
                        "failed to relativize package file".to_owned(),
                        false,
                    )
                })?
                .to_string_lossy()
                .replace('\\', "/");
            if relative != "MANIFEST.json" {
                files.push(relative);
            }
        }
    }
    Ok(())
}

fn validate_relative_path(path: &str) -> Result<(), AppError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.starts_with("\\\\")
        || path.contains('\0')
        || path.contains(":/")
        || path.contains(":\\")
    {
        return Err(unsafe_path(path));
    }
    for segment in path.replace('\\', "/").split('/') {
        if segment.is_empty() || segment == ".." {
            return Err(unsafe_path(path));
        }
    }
    Ok(())
}

fn unsafe_path(path: &str) -> AppError {
    AppError::new(
        ErrorCode::PackageUnsafePath,
        format!("unsafe package path: {path}"),
        false,
    )
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("portable package I/O failed: {error}"),
        false,
    )
}

fn zip_error(error: zip::result::ZipError) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("portable package archive failed: {error}"),
        false,
    )
}

fn parse_error(error: serde_json::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("portable package metadata parse failed: {error}"),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_project(root: &Path) {
        fs::create_dir_all(root.join(".lingbi")).expect("lingbi");
        fs::create_dir_all(root.join("chapters")).expect("chapters");
        fs::write(
            root.join(".lingbi/project.json"),
            r#"{"id":"project","name":"小说","schema_version":2}"#,
        )
        .expect("project");
        fs::write(root.join("chapters/chapter.md"), "第一章正文").expect("chapter");
    }

    #[test]
    fn package_round_trip_preserves_files() {
        let temp = TempDir::new().expect("temp dir");
        let project = temp.path().join("project");
        make_project(&project);
        let service = PortablePackageService;
        let package = temp.path().join("novel.lingbi");
        let destination = temp.path().join("restored");

        service.export_package(&project, &package).expect("export");
        service
            .import_package(&package, &destination)
            .expect("import");

        assert_eq!(
            fs::read_to_string(destination.join("chapters/chapter.md")).expect("chapter"),
            "第一章正文"
        );
        assert!(destination.join(".lingbi/project.json").exists());
    }

    #[test]
    fn unsafe_package_path_is_rejected() {
        let temp = TempDir::new().expect("temp dir");
        let package = temp.path().join("unsafe.lingbi");
        write_zip(
            &package,
            &PackageManifest {
                schema_version: PACKAGE_SCHEMA_VERSION,
                files: vec![PackageFile {
                    path: "../evil.txt".to_owned(),
                    sha256: hex_sha256(b"evil"),
                }],
            },
            &[("../evil.txt".to_owned(), b"evil".to_vec())],
        );

        let result = PortablePackageService.import_package(&package, &temp.path().join("restored"));

        assert!(matches!(
            result,
            Err(AppError {
                code: ErrorCode::PackageUnsafePath,
                ..
            })
        ));
    }

    #[test]
    fn checksum_mismatch_is_rejected() {
        let temp = TempDir::new().expect("temp dir");
        let package = temp.path().join("bad.lingbi");
        write_zip(
            &package,
            &PackageManifest {
                schema_version: PACKAGE_SCHEMA_VERSION,
                files: vec![PackageFile {
                    path: "chapters/chapter.md".to_owned(),
                    sha256: "bad".to_owned(),
                }],
            },
            &[("chapters/chapter.md".to_owned(), b"content".to_vec())],
        );

        let result = PortablePackageService.import_package(&package, &temp.path().join("restored"));

        assert!(matches!(
            result,
            Err(AppError {
                code: ErrorCode::PackageChecksumMismatch,
                ..
            })
        ));
    }

    fn write_zip(zip_path: &Path, manifest: &PackageManifest, files: &[(String, Vec<u8>)]) {
        let output = File::create(zip_path).expect("create");
        let mut zip = zip::ZipWriter::new(output);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("MANIFEST.json", options).expect("manifest");
        zip.write_all(&serde_json::to_vec(manifest).expect("manifest json"))
            .expect("write manifest");
        for (path, bytes) in files {
            zip.start_file(path, options).expect("start file");
            zip.write_all(bytes).expect("write file");
        }
        zip.finish().expect("finish");
    }
}
