use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    ProjectPathExists,
    ProjectNotFound,
    ProjectCorrupted,

    DocumentNotFound,
    DocumentAlreadyExists,
    DocumentConflict,

    AiNoApiKey,
    AiAuthFailed,
    AiRateLimited,
    AiTimeout,
    AiNetworkError,
    AiServerError,
    AiInvalidResponse,
    AiCancelled,

    MutationConflict,
    MutationNotApproved,
    CandidateStale,

    ImportUnsupportedFormat,
    PackageUnsafePath,
    PackageChecksumMismatch,

    Unauthorized,
    EntitlementExpired,
    EntitlementInvalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl AppError {
    pub const fn new(code: ErrorCode, message: String, retryable: bool) -> Self {
        Self {
            code,
            message,
            retryable,
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_round_trips_through_json() {
        let error = AppError::new(
            ErrorCode::ProjectPathExists,
            "project directory already exists".to_owned(),
            false,
        );

        let json = serde_json::to_string(&error).expect("serialize");
        let decoded: AppError = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded, error);
        assert!(json.contains("PROJECT_PATH_EXISTS"));
        assert_eq!(decoded.code, ErrorCode::ProjectPathExists);
        assert!(!decoded.retryable);
    }

    #[test]
    fn mutation_error_round_trips_through_json() {
        let error = AppError::new(
            ErrorCode::MutationNotApproved,
            "approval required".to_owned(),
            false,
        );

        let json = serde_json::to_string(&error).expect("serialize");
        let decoded: AppError = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded, error);
        assert!(json.contains("MUTATION_NOT_APPROVED"));
    }
}
