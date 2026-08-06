use lingbi_application::ProjectApplicationService;
use std::ffi::{CStr, CString, c_char};
use std::path::PathBuf;

#[unsafe(no_mangle)]
pub extern "C" fn lingbi_project_v2_schema_version() -> u32 {
    2
}

/// Opens a Project V2 and returns a JSON string.
///
/// # Safety
///
/// `root` must be a valid NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lingbi_open_project_json(root: *const c_char) -> *mut c_char {
    if root.is_null() {
        return error_json("root path is null");
    }
    let root = match unsafe { CStr::from_ptr(root) }.to_str() {
        Ok(root) => root,
        Err(_) => return error_json("root path is not valid UTF-8"),
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => return error_json(&format!("runtime failed: {error}")),
    };
    let result = runtime.block_on(async move {
        ProjectApplicationService::new()
            .open_project(PathBuf::from(root))
            .await
    });
    match result {
        Ok(snapshot) => {
            let value = serde_json::json!({
                "ok": true,
                "project": snapshot.project,
                "current_document": snapshot.current_document,
                "dirty": snapshot.dirty,
            });
            to_c_string(value.to_string())
        }
        Err(error) => error_json(&error.message),
    }
}

/// Frees a string returned by this FFI crate.
///
/// # Safety
///
/// `pointer` must be a pointer returned by this crate and not already freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lingbi_free_string(pointer: *mut c_char) {
    if !pointer.is_null() {
        unsafe {
            drop(CString::from_raw(pointer));
        }
    }
}

fn to_c_string(value: String) -> *mut c_char {
    CString::new(value)
        .expect("FFI string must not contain NUL")
        .into_raw()
}

fn error_json(message: &str) -> *mut c_char {
    to_c_string(
        serde_json::json!({
            "ok": false,
            "error": message,
        })
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lingbi_application::CreateProjectRequest;
    use tempfile::TempDir;

    #[test]
    fn schema_version_is_v2() {
        assert_eq!(lingbi_project_v2_schema_version(), 2);
    }

    #[test]
    fn open_project_json_returns_v2_session() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("novel");
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime
            .block_on(
                ProjectApplicationService::new().create_project(CreateProjectRequest {
                    name: "FFI测试".to_owned(),
                    root: root.clone(),
                }),
            )
            .expect("create project");

        let root_c = CString::new(root.to_string_lossy().as_ref()).expect("cstring");
        let result = unsafe { lingbi_open_project_json(root_c.as_ptr()) };
        let output = unsafe { CStr::from_ptr(result) }
            .to_string_lossy()
            .into_owned();
        unsafe { lingbi_free_string(result) };

        assert!(output.contains("\"schema_version\":2"));
        assert!(output.contains("FFI测试"));
    }
}
