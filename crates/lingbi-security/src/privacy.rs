use std::collections::HashSet;

const OPERATIONAL_FIELDS: &[&str] = &[
    "task_id",
    "provider_id",
    "model_id",
    "error_code",
    "duration_ms",
    "token_usage",
    "app_version",
];

const SENSITIVE_FIELD_HINTS: &[&str] = &[
    "manuscript",
    "prompt",
    "ai_response",
    "api_key",
    "project_dir",
    "file_name",
    "email",
];

#[derive(Debug, Clone, Default)]
pub struct PrivacyBaseline {
    pub crash_upload_opt_in: bool,
}

impl PrivacyBaseline {
    pub fn is_operational_field(field: &str) -> bool {
        OPERATIONAL_FIELDS.contains(&field.to_ascii_lowercase().as_str())
    }

    pub fn is_sensitive_field(field: &str) -> bool {
        let field = field.to_ascii_lowercase();
        SENSITIVE_FIELD_HINTS
            .iter()
            .any(|hint| field.contains(hint))
    }

    pub fn sanitize(&self, field: &str, value: &str) -> Option<(String, String)> {
        let normalized = field.to_ascii_lowercase();
        if !Self::is_operational_field(&normalized) {
            return None;
        }
        if Self::is_sensitive_field(&normalized) {
            return Some((field.to_owned(), "[redacted]".to_owned()));
        }
        Some((field.to_owned(), value.to_owned()))
    }

    pub fn sanitize_batch(&self, payload: &[(&str, &str)]) -> Vec<(String, String)> {
        let mut seen = HashSet::new();
        payload
            .iter()
            .filter_map(|(field, value)| self.sanitize(field, value))
            .filter(|(field, _)| seen.insert(field.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operational_fields_are_allowed() {
        let policy = PrivacyBaseline::default();
        let sanitized = policy.sanitize_batch(&[
            ("task_id", "task-1"),
            ("provider_id", "openai"),
            ("error_code", "AI_TIMEOUT"),
        ]);

        assert!(sanitized.contains(&("task_id".to_owned(), "task-1".to_owned())));
        assert!(sanitized.contains(&("provider_id".to_owned(), "openai".to_owned())));
    }

    #[test]
    fn sensitive_fields_never_cross_boundary() {
        let policy = PrivacyBaseline::default();
        let sanitized = policy.sanitize_batch(&[
            ("manuscript", "第一章正文"),
            ("prompt", "请续写"),
            ("ai_response", "候选正文"),
            ("api_key", "sk-secret"),
            ("project_dir", "/home/user/MyNovel"),
            ("file_name", "chapter.md"),
            ("email", "user@example.com"),
        ]);

        assert!(sanitized.is_empty());
    }

    #[test]
    fn crash_upload_requires_opt_in() {
        let policy = PrivacyBaseline::default();
        assert!(!policy.crash_upload_opt_in);
    }
}
