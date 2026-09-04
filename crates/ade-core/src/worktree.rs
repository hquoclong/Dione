//! Pure worktree helpers (M2 core, no git side effects yet).
//!
//! Conventions (cf. Orca / Hermes `-w` / Codex App):
//! - path: `<repo>/.ade-worktrees/<slug>`
//! - branch: `ade/<slug>`
//! - max ~15 managed worktrees, prune stale on startup.

use std::path::{Path, PathBuf};

pub const WORKTREE_DIR_NAME: &str = ".ade-worktrees";
pub const BRANCH_PREFIX: &str = "ade/";
pub const MAX_MANAGED_WORKTREES: usize = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeStatus {
    Creating,
    Working,
    NeedsYou,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeRecord {
    pub slug: String,
    pub branch: String,
    pub path: PathBuf,
    pub status: WorktreeStatus,
}

impl WorktreeRecord {
    pub fn new(repo: &Path, slug: &str) -> Option<Self> {
        let slug = normalize_slug(slug);
        if slug.is_empty() {
            return None;
        }
        Some(Self {
            branch: branch_name(&slug),
            path: worktree_path(repo, &slug),
            slug,
            status: WorktreeStatus::Creating,
        })
    }
}

/// Lowercase, alphanumeric + `-`, collapse runs, trim dashes, max 48 chars.
pub fn normalize_slug(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_dash = true; // trim leading dashes
    for c in raw.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
        if out.len() >= 48 {
            break;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

pub fn branch_name(slug: &str) -> String {
    format!("{BRANCH_PREFIX}{slug}")
}

pub fn worktree_path(repo: &Path, slug: &str) -> PathBuf {
    repo.join(WORKTREE_DIR_NAME).join(slug)
}

pub fn is_worktree_path(path: &Path) -> bool {
    path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .is_some_and(|s| s == WORKTREE_DIR_NAME)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_basic() {
        assert_eq!(normalize_slug("Feat Auth!"), "feat-auth");
        assert_eq!(normalize_slug("  --fix--api-- "), "fix-api");
        assert_eq!(normalize_slug(""), "");
        assert_eq!(normalize_slug("a"), "a");
    }

    #[test]
    fn record_paths() {
        let repo = Path::new("/repo");
        let r = WorktreeRecord::new(repo, "Feat Auth 3f2a").unwrap();
        assert_eq!(r.slug, "feat-auth-3f2a");
        assert_eq!(r.branch, "ade/feat-auth-3f2a");
        assert_eq!(r.path, PathBuf::from("/repo/.ade-worktrees/feat-auth-3f2a"));
    }

    #[test]
    fn rejects_empty_slug() {
        assert!(WorktreeRecord::new(Path::new("/r"), "!!!").is_none());
    }

    #[test]
    fn detects_worktree_path() {
        assert!(is_worktree_path(Path::new("/repo/.ade-worktrees/feat-x")));
        assert!(!is_worktree_path(Path::new("/repo/src/main.rs")));
    }
}
