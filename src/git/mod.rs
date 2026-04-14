mod branch;
pub mod config;
mod refs;
mod remote;
mod run;
mod worktree;

pub use branch::{
    checkout, checkout_in, checkout_new, current_branch, delete_branch_force_in,
    detect_default_branch, ensure_branch_allowed_in_linked_worktree, has_unpushed_commits,
    list_local_branches_with_upstream, stash_pop_in, stash_push_in,
};
pub use refs::{is_ancestor, resolve_ref, update_ref};
pub use remote::{fetch, fetch_all, list_remote_tracking_refs, pull};
pub use run::set_verbose;
pub use worktree::{
    OperationTarget, WorktreeEntry, add_worktree, find_main_worktree, is_main_worktree,
    is_worktree_clean, list_worktrees, read_worktree_root, remove_worktree,
    resolve_operation_target, worktree_path,
};
