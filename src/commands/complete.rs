use anyhow::Result;

use crate::cli::Cli;

pub fn run(_cli: &Cli, target: &str, prefix: Option<&str>) -> Result<()> {
    use super::util::not_in_git_repo_error;

    match target {
        "branches" => {
            let git_repo =
                crate::git::GitRepository::find().map_err(|_| not_in_git_repo_error())?;
            for name in git_repo.list_switchable_refs_matching_prefix(prefix.unwrap_or(""))? {
                println!("{name}");
            }
            Ok(())
        }
        other => Err(anyhow::anyhow!(
            "Unsupported completion target '{other}'. Supported targets: branches"
        )),
    }
}
