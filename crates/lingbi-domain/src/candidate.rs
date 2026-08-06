use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateStatus {
    Pending,
    Approved,
    Committed,
    Rejected,
    Stale,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: Uuid,
    pub project_id: Uuid,
    pub document_id: Uuid,
    pub instruction: String,
    pub base_revision: u64,
    pub base_content_hash: String,
    pub content: String,
    pub content_hash: String,
    pub provider_id: String,
    pub model_id: String,
    pub status: CandidateStatus,
    pub created_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub committed_at: Option<DateTime<Utc>>,
}

impl Candidate {
    pub fn approve(&mut self) {
        self.status = CandidateStatus::Approved;
        self.approved_at = Some(Utc::now());
    }

    pub fn commit(&mut self) {
        self.status = CandidateStatus::Committed;
        self.committed_at = Some(Utc::now());
    }

    pub fn reject(&mut self) {
        self.status = CandidateStatus::Rejected;
    }

    pub fn mark_stale(&mut self) {
        self.status = CandidateStatus::Stale;
    }

    pub fn mark_failed(&mut self) {
        self.status = CandidateStatus::Failed;
    }
}
