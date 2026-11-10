use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::xapi::{ApiError, Connection};

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VmAction {
    Start,
    Shutdown,
    Reboot,
    ForceShutdown,
    ForceReboot,
    Pause,
    Unpause,
    Suspend,
    Resume,
    Clone { name: String },
    Snapshot { name: String },
    Revert,
}

impl VmAction {
    pub fn allowed_operation(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Shutdown => "clean_shutdown",
            Self::Reboot => "clean_reboot",
            Self::ForceShutdown => "hard_shutdown",
            Self::ForceReboot => "hard_reboot",
            Self::Pause => "pause",
            Self::Unpause => "unpause",
            Self::Suspend => "suspend",
            Self::Resume => "resume",
            Self::Clone { .. } => "clone",
            Self::Snapshot { .. } => "snapshot",
            Self::Revert => "revert",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::Shutdown => "Shut down",
            Self::Reboot => "Reboot",
            Self::ForceShutdown => "Force shut down",
            Self::ForceReboot => "Force reboot",
            Self::Pause => "Pause",
            Self::Unpause => "Unpause",
            Self::Suspend => "Suspend",
            Self::Resume => "Resume",
            Self::Clone { .. } => "Clone",
            Self::Snapshot { .. } => "Take snapshot",
            Self::Revert => "Revert snapshot",
        }
    }

    fn params(&self, session: &str, reference: &str) -> Result<Value, ApiError> {
        Ok(match self {
            Self::Start | Self::Resume => json!([session, reference, false, false]),
            Self::Clone { name } | Self::Snapshot { name } => {
                if name.trim().is_empty() || name.chars().count() > 255 {
                    return Err(ApiError::Xen(
                        "Enter a name between 1 and 255 characters.".into(),
                    ));
                }
                json!([session, reference, name.trim()])
            }
            _ => json!([session, reference]),
        })
    }
}

impl Connection {
    /// Re-read the record before submitting. A stale UI cannot grant an operation.
    pub async fn begin_vm_action(&self, uuid: &str, action: &VmAction) -> Result<String, ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen(
                "This connection is read-only. Changes are blocked by the Rust backend.".into(),
            ));
        }
        let reference: String = self
            .request("VM.get_by_uuid", json!([self.session, uuid]))
            .await?;
        let record: Value = self
            .request("VM.get_record", json!([self.session, reference]))
            .await?;
        if record["is_control_domain"].as_bool() == Some(true) {
            return Err(ApiError::Xen(
                "Control domains cannot be managed through VM actions.".into(),
            ));
        }
        let snapshot = record["is_a_snapshot"].as_bool() == Some(true);
        if matches!(action, VmAction::Revert) != snapshot {
            return Err(ApiError::Xen(
                "This operation is not valid for the selected object.".into(),
            ));
        }
        if record["is_a_template"].as_bool() == Some(true) && !snapshot {
            return Err(ApiError::Xen(
                "Template provisioning is still being migrated.".into(),
            ));
        }
        let allowed = record["allowed_operations"].as_array().is_some_and(|ops| {
            ops.iter()
                .any(|op| op.as_str() == Some(action.allowed_operation()))
        });
        if !allowed {
            return Err(ApiError::Xen(
                "The host does not currently allow this operation. Refresh and check the VM state."
                    .into(),
            ));
        }
        let method = format!("Async.VM.{}", action.allowed_operation());
        self.request(&method, action.params(&self.session, &reference)?)
            .await.map_err(|error| match error {
                ApiError::Connection => ApiError::Xen("The submission outcome is unknown. The host may have accepted this operation. Refresh the VM and inspect the host tasks before submitting again.".into()),
                other => other,
            })
    }

    pub async fn task_record(&self, reference: &str) -> Result<RemoteTask, ApiError> {
        let record: Value = self
            .request("task.get_record", json!([self.session, reference]))
            .await?;
        let status = match record["status"].as_str() {
            Some("pending") => TaskStatus::Running,
            Some("success") => TaskStatus::Succeeded,
            Some("failure") => TaskStatus::Failed,
            Some("cancelled") => TaskStatus::Cancelled,
            _ => return Err(ApiError::Protocol),
        };
        let progress = record["progress"]
            .as_f64()
            .or_else(|| record["progress"].as_str().and_then(|p| p.parse().ok()))
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        // Error codes are useful without returning parameters that may contain private values.
        let code = record["error_info"][0]
            .as_str()
            .unwrap_or("Operation failed");
        let message = if status == TaskStatus::Failed
            && code.len() <= 128
            && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            code.to_owned()
        } else {
            String::new()
        };
        let can_cancel = record["allowed_operations"]
            .as_array()
            .is_some_and(|ops| ops.iter().any(|v| v.as_str() == Some("cancel")));
        Ok(RemoteTask {
            status,
            progress,
            message,
            can_cancel,
        })
    }

    pub async fn cancel_task(&self, reference: &str) -> Result<(), ApiError> {
        if self.settings.read_only {
            return Err(ApiError::Xen("This connection is read-only.".into()));
        }
        self.request::<Value>("task.cancel", json!([self.session, reference]))
            .await
            .map(|_| ())
    }

    pub async fn destroy_task(&self, reference: &str) {
        let _ = self
            .request::<Value>("task.destroy", json!([self.session, reference]))
            .await;
    }
}

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Unreachable,
}

impl TaskStatus {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

pub struct RemoteTask {
    pub status: TaskStatus,
    pub progress: f64,
    pub message: String,
    pub can_cancel: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn start_and_resume_never_force_a_vm() {
        assert_eq!(
            VmAction::Start.params("s", "v").unwrap(),
            json!(["s", "v", false, false])
        );
        assert_eq!(
            VmAction::Resume.params("s", "v").unwrap(),
            json!(["s", "v", false, false])
        );
    }
    #[test]
    fn empty_names_cannot_submit_clone_or_snapshot() {
        assert!(
            VmAction::Clone { name: "  ".into() }
                .params("s", "v")
                .is_err()
        );
        assert!(
            VmAction::Snapshot {
                name: "a".repeat(256)
            }
            .params("s", "v")
            .is_err()
        );
    }
}
