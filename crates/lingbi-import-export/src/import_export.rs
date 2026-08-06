use lingbi_application::DocumentApplicationService;
use lingbi_contracts::{AppError, ErrorCode};
use lingbi_domain::Document;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

pub struct ImportExportService {
    documents: Arc<DocumentApplicationService>,
}

impl ImportExportService {
    pub fn new(documents: Arc<DocumentApplicationService>) -> Self {
        Self { documents }
    }

    pub async fn import_text(
        &self,
        project_id: Uuid,
        source_path: &Path,
    ) -> Result<Document, AppError> {
        let extension = source_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if extension != "md" && extension != "txt" {
            return Err(AppError::new(
                ErrorCode::ImportUnsupportedFormat,
                format!("unsupported import extension: {extension}"),
                false,
            ));
        }
        let bytes = fs::read(source_path).map_err(io_error)?;
        let content = String::from_utf8_lossy(&bytes).into_owned();
        let title = source_path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "导入章节".to_owned());
        self.documents
            .create_document(project_id, title, content)
            .await
    }

    pub fn export_markdown(
        &self,
        content: impl AsRef<str>,
        save_path: &Path,
    ) -> Result<PathBuf, AppError> {
        write_text(content, save_path, "md")
    }

    pub fn export_txt(
        &self,
        content: impl AsRef<str>,
        save_path: &Path,
    ) -> Result<PathBuf, AppError> {
        write_text(content, save_path, "txt")
    }

    pub fn export_docx(
        &self,
        title: impl AsRef<str>,
        content: impl AsRef<str>,
        save_path: &Path,
    ) -> Result<PathBuf, AppError> {
        if save_path.extension().and_then(|ext| ext.to_str()) != Some("docx") {
            return Err(AppError::new(
                ErrorCode::ImportUnsupportedFormat,
                "export path must end with .docx".to_owned(),
                false,
            ));
        }
        if let Some(parent) = save_path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        let output = fs::File::create(save_path).map_err(io_error)?;
        let mut zip = zip::ZipWriter::new(output);
        let options = zip::write::SimpleFileOptions::default();

        let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;
        let root_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;
        let document_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#;
        let document = build_docx_xml(title.as_ref(), content.as_ref());

        zip.start_file("[Content_Types].xml", options)
            .map_err(zip_error)?;
        zip.write_all(content_types.as_bytes()).map_err(io_error)?;
        zip.start_file("_rels/.rels", options).map_err(zip_error)?;
        zip.write_all(root_rels.as_bytes()).map_err(io_error)?;
        zip.start_file("word/_rels/document.xml.rels", options)
            .map_err(zip_error)?;
        zip.write_all(document_rels.as_bytes()).map_err(io_error)?;
        zip.start_file("word/document.xml", options)
            .map_err(zip_error)?;
        zip.write_all(document.as_bytes()).map_err(io_error)?;
        zip.finish().map_err(zip_error)?;
        Ok(save_path.to_path_buf())
    }
}

fn write_text(
    content: impl AsRef<str>,
    save_path: &Path,
    extension: &str,
) -> Result<PathBuf, AppError> {
    if save_path.extension().and_then(|ext| ext.to_str()) != Some(extension) {
        return Err(AppError::new(
            ErrorCode::ImportUnsupportedFormat,
            format!("export path must end with .{extension}"),
            false,
        ));
    }
    if let Some(parent) = save_path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    fs::write(save_path, content.as_ref()).map_err(io_error)?;
    Ok(save_path.to_path_buf())
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("import/export I/O failed: {error}"),
        false,
    )
}

fn zip_error(error: zip::result::ZipError) -> AppError {
    AppError::new(
        ErrorCode::ProjectCorrupted,
        format!("DOCX archive failed: {error}"),
        false,
    )
}

fn build_docx_xml(title: &str, content: &str) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
    );
    xml.push_str(&format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Title\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        xml_escape(title)
    ));
    for line in content.lines() {
        xml.push_str(&format!(
            "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            xml_escape(line)
        ));
    }
    xml.push_str("</w:body></w:document>");
    xml
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn imports_markdown_and_txt() {
        let temp = TempDir::new().expect("temp dir");
        let documents = Arc::new(DocumentApplicationService::new(temp.path().join("project")));
        let service = ImportExportService::new(documents.clone());
        let md = temp.path().join("source.md");
        let txt = temp.path().join("source.txt");
        fs::write(&md, "# 导入").expect("md");
        fs::write(&txt, "导入正文").expect("txt");

        let md_doc = service
            .import_text(Uuid::new_v4(), &md)
            .await
            .expect("import md");
        let txt_doc = service
            .import_text(Uuid::new_v4(), &txt)
            .await
            .expect("import txt");

        assert_eq!(md_doc.title, "source");
        assert_eq!(txt_doc.title, "source");
    }

    #[tokio::test]
    async fn rejects_unsupported_import() {
        let temp = TempDir::new().expect("temp dir");
        let documents = Arc::new(DocumentApplicationService::new(temp.path().join("project")));
        let service = ImportExportService::new(documents);
        let pdf = temp.path().join("source.pdf");
        fs::write(&pdf, "pdf").expect("pdf");

        let result = service.import_text(Uuid::new_v4(), &pdf).await;

        assert!(matches!(
            result,
            Err(AppError {
                code: ErrorCode::ImportUnsupportedFormat,
                ..
            })
        ));
    }

    #[test]
    fn exports_markdown_and_txt() {
        let temp = TempDir::new().expect("temp dir");
        let documents = Arc::new(DocumentApplicationService::new(temp.path().join("project")));
        let service = ImportExportService::new(documents);
        let md = temp.path().join("chapter.md");
        let txt = temp.path().join("chapter.txt");

        service.export_markdown("# 正文", &md).expect("md");
        service.export_txt("正文", &txt).expect("txt");

        assert_eq!(fs::read_to_string(md).expect("read md"), "# 正文");
        assert_eq!(fs::read_to_string(txt).expect("read txt"), "正文");
    }

    #[test]
    fn exports_docx_with_readable_xml() {
        let temp = TempDir::new().expect("temp dir");
        let documents = Arc::new(DocumentApplicationService::new(temp.path().join("project")));
        let service = ImportExportService::new(documents);
        let docx = temp.path().join("chapter.docx");

        service
            .export_docx("第一章", "你好 <世界>", &docx)
            .expect("docx");

        let file = fs::File::open(&docx).expect("open docx");
        let mut archive = zip::ZipArchive::new(file).expect("zip");
        let mut document = archive.by_name("word/document.xml").expect("document.xml");
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut document, &mut xml).expect("read xml");
        assert!(xml.contains("第一章"));
        assert!(xml.contains("你好 &lt;世界&gt;"));
    }
}
