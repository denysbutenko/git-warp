#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DoctorHookSeverity {
    Healthy,
    Partial,
    Missing,
}

pub(super) struct DoctorHooksSummary {
    pub(super) severity: DoctorHookSeverity,
    pub(super) detail: String,
    pub(super) next_steps: Vec<String>,
}

pub(super) fn doctor_hooks_summary() -> DoctorHooksSummary {
    use crate::hooks::{HookState, HooksManager};

    let diagnoses = match HooksManager::diagnose("all") {
        Ok(d) => d,
        Err(_) => {
            return DoctorHooksSummary {
                severity: DoctorHookSeverity::Missing,
                detail: "unable to inspect hook configuration".to_string(),
                next_steps: vec![
                    "Run `warp hooks-status --runtime all` to investigate.".to_string(),
                ],
            };
        }
    };

    let mut healthy: Vec<String> = Vec::new();
    let mut partial: Vec<String> = Vec::new();
    let mut conflicting: Vec<String> = Vec::new();
    let mut next_steps: Vec<String> = Vec::new();

    for d in &diagnoses {
        let scope_label = match d.scope {
            crate::hooks::HookScope::User => "user",
            crate::hooks::HookScope::Project => "project",
        };
        let runtime_label = match d.runtime {
            crate::hooks::HookRuntime::Claude => "claude",
            crate::hooks::HookRuntime::Codex => "codex",
        };
        let combined = format!("{scope_label} {runtime_label}");

        match d.state {
            HookState::Complete => healthy.push(combined),
            HookState::Partial => {
                partial.push(combined);
                next_steps.push(format!(
                    "Run `{}` to repair partial {} {} hooks.",
                    d.install_command(),
                    scope_label,
                    runtime_label
                ));
            }
            HookState::Conflicting => {
                conflicting.push(combined);
                next_steps.push(format!(
                    "Run `{}` to deduplicate {} {} hooks.",
                    d.install_command(),
                    scope_label,
                    runtime_label
                ));
            }
            HookState::Missing | HookState::NotConfigured => {}
        }
    }

    if healthy.is_empty() && partial.is_empty() && conflicting.is_empty() {
        return DoctorHooksSummary {
            severity: DoctorHookSeverity::Missing,
            detail: "no user or project git-warp hooks found".to_string(),
            next_steps: vec![
                "Run `warp hooks-install --level user --runtime all` to enable live agent monitoring.".to_string(),
            ],
        };
    }

    if !partial.is_empty() || !conflicting.is_empty() {
        let mut detail_parts = Vec::new();
        if !healthy.is_empty() {
            detail_parts.push(format!("complete: {}", healthy.join(", ")));
        }
        if !partial.is_empty() {
            detail_parts.push(format!("partial: {}", partial.join(", ")));
        }
        if !conflicting.is_empty() {
            detail_parts.push(format!("conflicting: {}", conflicting.join(", ")));
        }
        return DoctorHooksSummary {
            severity: DoctorHookSeverity::Partial,
            detail: detail_parts.join("; "),
            next_steps,
        };
    }

    DoctorHooksSummary {
        severity: DoctorHookSeverity::Healthy,
        detail: format!("complete: {}", healthy.join(", ")),
        next_steps,
    }
}
