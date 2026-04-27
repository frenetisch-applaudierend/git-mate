const COPY_BLACKLIST: &[&str] = &[
    "node_modules",
    "target",
    ".gradle",
    ".m2",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".svelte-kit",
    "obj",
    "bin",
    "vendor",
    ".terraform",
    "coverage",
    ".nyc_output",
    ".cache",
];

fn copied_local_config_files_message(copied: usize) -> String {
    let noun = if copied == 1 { "file" } else { "files" };
    format!("Copied {copied} local config {noun} to worktree")
}

fn local_config_paths(src: &std::path::Path) -> Result<Option<Vec<String>>, String> {
    let src_str = src.to_str().ok_or("source path is not valid UTF-8")?;
    let output = std::process::Command::new("git")
        .args([
            "-C",
            src_str,
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
            "-z",
        ])
        .output()
        .map_err(|e| format!("failed to list ignored files: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        crate::output::info(&format!(
            "skipping local config copy: git ls-files failed: {}",
            stderr.trim()
        ));
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(Some(
        stdout
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
    ))
}

fn path_is_blacklisted(rel_path: &str) -> bool {
    rel_path
        .trim_end_matches('/')
        .split('/')
        .any(|c| COPY_BLACKLIST.contains(&c))
}

pub fn copy_ignored_files(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    let Some(local_config_paths) = local_config_paths(src)? else {
        return Ok(());
    };
    let mut copied = 0usize;
    for rel_path in local_config_paths {
        if path_is_blacklisted(&rel_path) {
            continue;
        }
        if rel_path.ends_with('/') {
            copied += copy_dir(src, dst, rel_path.trim_end_matches('/'));
        } else {
            copied += copy_file(src, dst, &rel_path) as usize;
        }
    }
    if copied > 0 {
        crate::output::info(&copied_local_config_files_message(copied));
    }
    Ok(())
}

pub fn has_local_config_files(src: &std::path::Path) -> Result<bool, String> {
    Ok(local_config_paths(src)?
        .map(|paths| paths.into_iter().any(|path| !path_is_blacklisted(&path)))
        .unwrap_or(false))
}

fn copy_file(src: &std::path::Path, dst: &std::path::Path, rel_path: &str) -> bool {
    let src_file = src.join(rel_path);
    let dst_file = dst.join(rel_path);
    if dst_file.exists() {
        return false;
    }
    if let Some(parent) = dst_file.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return false;
    }
    if let Err(e) = std::fs::copy(&src_file, &dst_file) {
        crate::output::info(&format!("could not copy {rel_path}: {e}"));
        return false;
    }
    true
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path, rel_dir: &str) -> usize {
    let src_dir = src.join(rel_dir);
    let Ok(entries) = std::fs::read_dir(&src_dir) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let rel_path = format!("{rel_dir}/{}", name.to_string_lossy());
        if path_is_blacklisted(&rel_path) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            count += copy_dir(src, dst, &rel_path);
        } else if file_type.is_file() {
            count += copy_file(src, dst, &rel_path) as usize;
        }
    }
    count
}

pub fn remove_empty_parent_dirs(path: &std::path::Path, stop_at: &std::path::Path) {
    let mut current = path.to_path_buf();
    loop {
        current = match current.parent() {
            Some(p) => p.to_path_buf(),
            None => break,
        };
        if current == stop_at || !current.starts_with(stop_at) {
            break;
        }
        if std::fs::remove_dir(&current).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn copied_local_config_files_message_uses_singular() {
        assert_eq!(
            super::copied_local_config_files_message(1),
            "Copied 1 local config file to worktree"
        );
    }

    #[test]
    fn copied_local_config_files_message_uses_plural() {
        assert_eq!(
            super::copied_local_config_files_message(87),
            "Copied 87 local config files to worktree"
        );
    }
}
