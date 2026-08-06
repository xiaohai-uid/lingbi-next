use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub order: i64,
    pub revision: u64,
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Document {
    pub fn physical_path(&self) -> PathBuf {
        PathBuf::from("chapters").join(format!("{}.md", self.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(title: &str) -> Document {
        let now = Utc::now();
        Document {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            title: title.to_owned(),
            order: 0,
            revision: 0,
            content_hash: String::new(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn same_title_documents_have_different_uuid_paths() {
        let first = document("第一章");
        let second = document("第一章");

        assert_ne!(first.id, second.id);
        assert_ne!(first.physical_path(), second.physical_path());
        assert!(!first.physical_path().to_string_lossy().contains("第一章"));
    }
}
