use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeedbackSettings {
    /// When the feedback form was last shown to the user
    pub last_feedback_prompt: Option<DateTime<Utc>>,
}

impl FeedbackSettings {
    pub fn should_show_feedback(&self) -> bool {
        match self.last_feedback_prompt {
            None => true,
            Some(last_time) => {
                let now = Utc::now();
                let duration = now.signed_duration_since(last_time);
                // Show once a month (30 days)
                duration.num_days() >= 30
            }
        }
    }

    pub fn update_last_shown(&mut self) {
        self.last_feedback_prompt = Some(Utc::now());
    }
}
