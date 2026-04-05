pub mod config;
mod run;
mod branch;
mod remote;
mod refs;
mod worktree;

pub use branch::{
    checkout, checkout_in, checkout_new, current_branch, delete_branch_force_in,
    detect_default_branch, ensure_branch_allowed_in_linked_worktree, has_unpushed_commits,
    list_local_branches_with_upstream,
};
pub use remote::{fetch, fetch_all, list_remote_tracking_refs, pull};
pub use refs::{is_ancestor, resolve_ref, stash_pop_in, stash_push, update_ref};
pub use run::set_verbose;
pub use worktree::{
    add_worktree, find_main_worktree,
    find_worktree_for_branch, is_main_worktree, is_worktree_clean, list_worktrees,
    read_worktree_root, remove_worktree, resolve_operation_target, worktree_path, OperationTarget,
    WorktreeEntry,
};
