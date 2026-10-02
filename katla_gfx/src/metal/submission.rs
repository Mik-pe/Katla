//! Completion feedback shared with Metal's serial feedback dispatcher.

use std::sync::{Arc, Condvar, Mutex};

pub(crate) use super::diagnostics::CommitError;
use crate::error::RendererError;

#[derive(Clone, Debug, Default)]
pub(crate) struct CommitFeedback {
    pub(crate) gpu_start: f64,
    pub(crate) gpu_end: f64,
    pub(crate) error: Option<CommitError>,
}

#[derive(Default)]
struct CompletionState {
    submitted: bool,
    feedback: Option<CommitFeedback>,
}

#[derive(Clone, Default)]
pub(crate) struct SubmissionCompletion {
    state: Arc<(Mutex<CompletionState>, Condvar)>,
}

impl SubmissionCompletion {
    pub(crate) fn mark_submitted(&self) -> bool {
        let mut state = self.state.0.lock().expect("submission mutex poisoned");
        if state.submitted {
            return false;
        }
        state.submitted = true;
        true
    }

    pub(crate) fn finish(&self, feedback: CommitFeedback) {
        let mut state = self.state.0.lock().expect("submission mutex poisoned");
        state.feedback = Some(feedback);
        self.state.1.notify_all();
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.state
            .0
            .lock()
            .expect("submission mutex poisoned")
            .feedback
            .is_some()
    }

    pub(crate) fn wait(&self) -> Option<CommitFeedback> {
        let mut state = self.state.0.lock().expect("submission mutex poisoned");
        while state.submitted && state.feedback.is_none() {
            state = self.state.1.wait(state).expect("submission mutex poisoned");
        }
        state.feedback.clone()
    }

    pub(crate) fn result(&self, label: &str) -> Result<CommitFeedback, RendererError> {
        let feedback = self.wait().ok_or_else(|| {
            RendererError::InvalidOperation("command buffer has not been submitted".into())
        })?;
        if let Some(error) = &feedback.error {
            return Err(error.renderer_error(label));
        }
        Ok(feedback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_submission_rejects_duplicate_commit() {
        let completion = SubmissionCompletion::default();
        assert!(completion.mark_submitted());
        assert!(!completion.mark_submitted());
        assert!(!completion.is_complete());
        completion.finish(CommitFeedback::default());
        assert!(completion.is_complete());
    }
    #[test]
    fn test_feedback_waits_for_exact_submission() {
        let first = SubmissionCompletion::default();
        let second = SubmissionCompletion::default();
        first.mark_submitted();
        second.mark_submitted();
        first.finish(CommitFeedback::default());
        assert!(first.is_complete());
        assert!(!second.is_complete());
        let callback = second.clone();
        std::thread::spawn(move || {
            callback.finish(CommitFeedback {
                gpu_start: 4.0,
                gpu_end: 4.003,
                error: None,
            })
        });
        let feedback = second.result("slot.1.frame.2").unwrap();
        assert!((feedback.gpu_end - feedback.gpu_start - 0.003).abs() < 1e-12);
    }
    #[test]
    fn test_feedback_native_failure_is_typed() {
        let completion = SubmissionCompletion::default();
        completion.mark_submitted();
        completion.finish(CommitFeedback {
            error: Some(CommitError {
                code: 3,
                domain: "MTL4CommandQueueErrorDomain".into(),
                description: "out of memory".into(),
            }),
            ..Default::default()
        });
        let RendererError::GpuExecutionFailed(error) =
            completion.result("slot.2.frame.5").unwrap_err()
        else {
            panic!("typed GPU error");
        };
        assert_eq!(error.code, Some(3));
        assert_eq!(error.label, "slot.2.frame.5");
    }
}
