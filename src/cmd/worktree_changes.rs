pub(crate) struct PreparedStash {
    branch: String,
}

impl PreparedStash {
    fn new(branch: &str) -> Self {
        Self {
            branch: branch.to_string(),
        }
    }

    fn pop_into(&self, path: &std::path::Path) -> Result<(), String> {
        let path_str = path.to_str().ok_or("worktree path is not valid UTF-8")?;
        crate::git::stash_pop_in(path_str, "stash@{0}")
    }
}

pub(crate) fn stash_changes(
    path: &std::path::Path,
    branch: &str,
    label: &str,
    stash_message: &str,
) -> Result<PreparedStash, String> {
    let path_str = path.to_str().ok_or("worktree path is not valid UTF-8")?;
    crate::git::stash_push_in(path_str, stash_message)
        .map_err(|e| format!("failed to stash changes in {label}: {e}"))?;
    Ok(PreparedStash::new(branch))
}

pub(crate) fn restore_source_stash(
    stash: Option<&PreparedStash>,
    path: &std::path::Path,
    failure_message: String,
) -> String {
    match stash {
        Some(stash) => stash
            .pop_into(path)
            .err()
            .map_or(failure_message.clone(), |e| {
                format!(
                    "{failure_message}; also failed to restore stashed changes for '{}': {e}",
                    stash.branch
                )
            }),
        None => failure_message,
    }
}

pub(crate) fn finish_with_stash_reapply(
    stash: PreparedStash,
    apply_path: &std::path::Path,
    success_message: &str,
    cd_path: &std::path::Path,
) -> Result<(), String> {
    match stash.pop_into(apply_path) {
        Ok(()) => {
            crate::output::success(success_message);
            crate::shell_protocol::emit_cd(cd_path);
            Ok(())
        }
        Err(e) => {
            crate::shell_protocol::emit_cd(cd_path);
            Err(format!(
                "{success_message}, but failed to reapply stashed changes for '{}': {e}; the stash entry was kept",
                stash.branch
            ))
        }
    }
}
