use super::HooksManager;
use super::types::{EXPECTED_EVENTS, HookInstallLevel, HookRemoveLevel, HookRuntime, HookScope};
use crate::error::Result;

impl HooksManager {
    pub fn install_hooks(level: HookInstallLevel, runtime: &str) -> Result<()> {
        let runtimes = HookRuntime::parse_many(runtime)?;

        match level {
            HookInstallLevel::Console => {
                for (index, runtime) in runtimes.iter().enumerate() {
                    if index > 0 {
                        println!();
                    }
                    println!("Add this to your {} hook config:", runtime.display_name());
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&Self::get_hooks_config(*runtime))?
                    );
                }
                Ok(())
            }
            HookInstallLevel::User => {
                for runtime in runtimes {
                    let settings_path = runtime.user_settings_path()?;
                    Self::merge_hooks_into_settings(settings_path, runtime)?;
                }
                Ok(())
            }
            HookInstallLevel::Project => {
                for runtime in runtimes {
                    let settings_path = runtime.project_settings_path()?;
                    Self::merge_hooks_into_settings(settings_path, runtime)?;
                }
                Ok(())
            }
        }
    }

    pub fn remove_hooks(level: HookRemoveLevel, runtime: &str) -> Result<()> {
        let runtimes = HookRuntime::parse_many(runtime)?;

        match level {
            HookRemoveLevel::User => {
                for runtime in runtimes {
                    Self::remove_hooks_from_settings(runtime.user_settings_path()?, runtime)?;
                }
                Ok(())
            }
            HookRemoveLevel::Project => {
                for runtime in runtimes {
                    Self::remove_hooks_from_settings(runtime.project_settings_path()?, runtime)?;
                }
                Ok(())
            }
        }
    }

    pub fn show_hooks_status(runtime: &str) -> Result<()> {
        let runtimes = HookRuntime::parse_many(runtime)?;

        println!("🔧 Git-Warp Agent Integration Status");
        println!("====================================");
        println!("Expected events: {}", EXPECTED_EVENTS.join(", "));

        let mut repair_steps: Vec<String> = Vec::new();
        let mut any_complete = false;

        for (index, runtime) in runtimes.iter().enumerate() {
            if index > 0 {
                println!();
            }

            println!("\n{}:", runtime.display_name());

            for scope in [HookScope::User, HookScope::Project] {
                let diagnosis = Self::diagnose_scope(*runtime, scope);
                Self::print_diagnosis(&diagnosis);
                if diagnosis.is_healthy() {
                    any_complete = true;
                } else {
                    repair_steps.push(diagnosis.install_command());
                }
            }
        }

        println!("\n📖 Repair guidance:");
        if repair_steps.is_empty() {
            println!("   All checked scopes look healthy. No action needed.");
        } else {
            for step in &repair_steps {
                println!("   {step}");
            }
            if !any_complete {
                println!(
                    "   warp hooks-install --level user --runtime all  # bootstrap both runtimes at user level"
                );
            }
        }

        Ok(())
    }
}
