use lingbi_ai::AiError;
use lingbi_contracts::{AppError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenerationState {
    Idle,
    Preparing,
    Connecting,
    Generating,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerationTask {
    pub id: Uuid,
    pub chapter_id: Uuid,
    pub instruction: String,
    pub state: GenerationState,
    pub error: Option<AiError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationRequest {
    pub chapter_id: Uuid,
    pub instruction: String,
}

#[derive(Default)]
pub struct GenerationManager {
    tasks: Mutex<HashMap<Uuid, GenerationTask>>,
}

impl GenerationManager {
    pub fn start_generation(&self, request: GenerationRequest) -> Uuid {
        let id = Uuid::new_v4();
        let task = GenerationTask {
            id,
            chapter_id: request.chapter_id,
            instruction: request.instruction,
            state: GenerationState::Preparing,
            error: None,
        };
        self.tasks
            .lock()
            .expect("generation manager lock")
            .insert(id, task);
        id
    }

    pub fn generation_status(&self, task_id: Uuid) -> Option<GenerationState> {
        self.tasks
            .lock()
            .expect("generation manager lock")
            .get(&task_id)
            .map(|task| task.state)
    }

    pub fn cancel_generation(&self, task_id: Uuid) -> Result<(), AppError> {
        let mut tasks = self.tasks.lock().expect("generation manager lock");
        let task = tasks.get_mut(&task_id).ok_or_else(unknown_task)?;
        if matches!(
            task.state,
            GenerationState::Completed | GenerationState::Failed | GenerationState::Cancelled
        ) {
            return Err(AppError::new(
                ErrorCode::MutationConflict,
                "generation is already terminal".to_owned(),
                false,
            ));
        }
        task.state = GenerationState::Cancelled;
        task.error = None;
        Ok(())
    }

    pub fn connecting(&self, task_id: Uuid) -> Result<(), AppError> {
        self.transition(task_id, GenerationState::Connecting)
    }

    pub fn generating(&self, task_id: Uuid) -> Result<(), AppError> {
        self.transition(task_id, GenerationState::Generating)
    }

    pub fn complete_success(&self, task_id: Uuid) -> Result<(), AppError> {
        self.transition(task_id, GenerationState::Completed)
    }

    pub fn complete_failure(&self, task_id: Uuid, error: AiError) -> Result<(), AppError> {
        let mut tasks = self.tasks.lock().expect("generation manager lock");
        let task = tasks.get_mut(&task_id).ok_or_else(unknown_task)?;
        task.state = GenerationState::Failed;
        task.error = Some(error);
        Ok(())
    }

    fn transition(&self, task_id: Uuid, state: GenerationState) -> Result<(), AppError> {
        let mut tasks = self.tasks.lock().expect("generation manager lock");
        let task = tasks.get_mut(&task_id).ok_or_else(unknown_task)?;
        if matches!(
            task.state,
            GenerationState::Completed | GenerationState::Failed | GenerationState::Cancelled
        ) {
            return Err(AppError::new(
                ErrorCode::MutationConflict,
                "generation is already terminal".to_owned(),
                false,
            ));
        }
        task.state = state;
        Ok(())
    }
}

fn unknown_task() -> AppError {
    AppError::new(
        ErrorCode::DocumentNotFound,
        "generation task not found".to_owned(),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn start_then_cancel_returns_cancelled() {
        let manager = GenerationManager::default();
        let task_id = manager.start_generation(GenerationRequest {
            chapter_id: Uuid::new_v4(),
            instruction: "write".to_owned(),
        });

        assert_eq!(
            manager.generation_status(task_id),
            Some(GenerationState::Preparing)
        );
        manager.cancel_generation(task_id).expect("cancel");
        assert_eq!(
            manager.generation_status(task_id),
            Some(GenerationState::Cancelled)
        );
    }

    #[tokio::test]
    async fn provider_error_marks_failed_without_completion() {
        let manager = GenerationManager::default();
        let task_id = manager.start_generation(GenerationRequest {
            chapter_id: Uuid::new_v4(),
            instruction: "write".to_owned(),
        });

        manager
            .complete_failure(task_id, AiError::AuthFailed)
            .expect("failure");

        assert_eq!(
            manager.generation_status(task_id),
            Some(GenerationState::Failed)
        );
        assert!(
            manager
                .tasks
                .lock()
                .expect("lock")
                .get(&task_id)
                .expect("task")
                .error
                .is_some()
        );
    }

    #[tokio::test]
    async fn empty_stream_is_failure() {
        let manager = GenerationManager::default();
        let task_id = manager.start_generation(GenerationRequest {
            chapter_id: Uuid::new_v4(),
            instruction: "write".to_owned(),
        });

        manager
            .complete_failure(task_id, AiError::InvalidResponse)
            .expect("failure");

        assert_eq!(
            manager.generation_status(task_id),
            Some(GenerationState::Failed)
        );
    }
}
