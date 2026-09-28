use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl ExecutionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Queued, Self::Running)
                | (Self::Queued, Self::Cancelled)
                | (Self::Running, Self::Succeeded)
                | (Self::Running, Self::Failed)
                | (Self::Running, Self::Cancelled)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_allows_valid_execution_transitions() {
        assert!(ExecutionStatus::Queued.can_transition_to(ExecutionStatus::Running));
        assert!(ExecutionStatus::Running.can_transition_to(ExecutionStatus::Succeeded));
        assert!(ExecutionStatus::Running.can_transition_to(ExecutionStatus::Failed));
        assert!(ExecutionStatus::Running.can_transition_to(ExecutionStatus::Cancelled));
        assert!(!ExecutionStatus::Succeeded.can_transition_to(ExecutionStatus::Running));
        assert!(!ExecutionStatus::Failed.can_transition_to(ExecutionStatus::Running));
    }
}
