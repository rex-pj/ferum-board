use async_trait::async_trait;

use ferum_application::ports::{ForumJob, JobQueue};
use ferum_application::shared::AppError;

mockall::mock! {
    pub JobQueue {}

    #[async_trait]
    impl JobQueue for JobQueue {
        async fn enqueue(&self, job: ForumJob) -> Result<(), AppError>;
    }
}

/// No-op JobQueue that silently accepts all jobs. Use when job enqueuing is a
/// fire-and-forget side effect not relevant to the test's assertions.
pub struct NoopJobQueue;

#[async_trait]
impl JobQueue for NoopJobQueue {
    async fn enqueue(&self, _job: ForumJob) -> Result<(), AppError> { Ok(()) }
}
