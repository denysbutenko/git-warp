use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SwitchStepStatus {
    Done,
    Skipped,
    Warning,
}

pub(super) struct SwitchStep {
    label: &'static str,
    status: SwitchStepStatus,
    detail: String,
}

impl SwitchStep {
    fn print(&self) {
        let icon = match self.status {
            SwitchStepStatus::Done => "✅",
            SwitchStepStatus::Skipped => "↪️ ",
            SwitchStepStatus::Warning => "⚠️ ",
        };

        println!("{} {}: {}", icon, self.label, self.detail);
    }
}

pub(super) struct SwitchOutcomeReport {
    worktree_path: PathBuf,
    steps: Vec<SwitchStep>,
}

impl SwitchOutcomeReport {
    pub(super) fn new(worktree_path: PathBuf) -> Self {
        Self {
            worktree_path,
            steps: Vec::new(),
        }
    }

    pub(super) fn done(&mut self, label: &'static str, detail: impl Into<String>) {
        self.push(label, SwitchStepStatus::Done, detail);
    }

    pub(super) fn skipped(&mut self, label: &'static str, detail: impl Into<String>) {
        self.push(label, SwitchStepStatus::Skipped, detail);
    }

    pub(super) fn warned(&mut self, label: &'static str, detail: impl Into<String>) {
        self.push(label, SwitchStepStatus::Warning, detail);
    }

    fn push(&mut self, label: &'static str, status: SwitchStepStatus, detail: impl Into<String>) {
        let step = SwitchStep {
            label,
            status,
            detail: detail.into(),
        };
        step.print();
        self.steps.push(step);
    }

    fn has_warnings(&self) -> bool {
        self.steps
            .iter()
            .any(|step| step.status == SwitchStepStatus::Warning)
    }

    pub(super) fn finish(&self) {
        if self.has_warnings() {
            println!("⚠️  Switch incomplete: {}", self.worktree_path.display());
            println!("💡 Run: cd '{}'", self.worktree_path.display());
        } else {
            println!("✅ Switch complete: {}", self.worktree_path.display());
        }
    }
}
