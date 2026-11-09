mod base;
mod branch;
pub mod config;
mod integrate;
mod merge;
mod push;
mod refs;
mod remote;
mod run;
mod worktree;

pub use base::{
    Base, BaseSource, DeletedBranch, Detection, behind, behind_base, branch_ref,
    default_base_branch, detect_base,
};
pub use branch::{
    checkout, checkout_force_in, checkout_in, checkout_new_in, current_branch,
    delete_branch_force_in, detect_default_branch, ensure_branch_allowed_in_linked_worktree,
    has_unpushed_commits, is_valid_branch_name, list_local_branches_with_upstream,
    local_branch_for_ref, merge_ff_only_in, stash_pop_in, stash_push_in, unset_upstream,
};
pub use integrate::{Integration, Merge, Rebase, merge_into_branch, rebase_branch};
pub use merge::{MergedAt, content_merged_into, patches_already_in, reset_hard_in};
pub use push::{push_branch, push_target};
pub use refs::{
    commit_exists, commit_summary, count_commits, is_ancestor, resolve_ref, update_ref,
    update_ref_from,
};
pub use remote::{fetch, fetch_all, list_remote_tracking_refs, list_remotes};
pub use run::{is_verbose, set_verbose};
pub use worktree::{
    OperationTarget, WorktreeEntry, add_worktree, branch_being_rebased, current_worktree_root,
    expand_tilde, find_main_worktree, is_main_worktree, is_worktree_clean, list_worktrees,
    operation_in_progress, prune_worktrees, read_worktree_root, remove_worktree,
    resolve_operation_target, worktree_for_branch, worktree_path,
};
