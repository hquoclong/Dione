//! Minimal M0 store: worktrees + errors only.
//! Sessions/messages/diffs arrive in M1; fleet dashboard reads this in M2.

use std::collections::{BTreeMap, VecDeque};

use crate::worktree::WorktreeRecord;

#[derive(Debug, Clone, Default)]
pub struct Store {
    pub worktrees: BTreeMap<String, WorktreeRecord>,
    pub active_worktree: Option<String>,
    pub errors: VecDeque<String>,
}

impl Store {
    pub fn upsert_worktree(&mut self, record: WorktreeRecord) {
        if self.active_worktree.is_none() {
            self.active_worktree = Some(record.slug.clone());
        }
        self.worktrees.insert(record.slug.clone(), record);
    }

    pub fn remove_worktree(&mut self, slug: &str) -> bool {
        let removed = self.worktrees.remove(slug).is_some();
        if self.active_worktree.as_deref() == Some(slug) {
            self.active_worktree = self.worktrees.keys().next().cloned();
        }
        removed
    }

    pub fn set_active(&mut self, slug: &str) -> bool {
        if self.worktrees.contains_key(slug) {
            self.active_worktree = Some(slug.to_string());
            true
        } else {
            false
        }
    }

    pub fn push_error(&mut self, msg: impl Into<String>) {
        self.errors.push_back(msg.into());
        while self.errors.len() > 20 {
            self.errors.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn record(slug: &str) -> WorktreeRecord {
        WorktreeRecord::new(Path::new("/repo"), slug).unwrap()
    }

    #[test]
    fn upsert_sets_first_active() {
        let mut s = Store::default();
        s.upsert_worktree(record("feat-a"));
        assert_eq!(s.active_worktree.as_deref(), Some("feat-a"));
        s.upsert_worktree(record("feat-b"));
        assert_eq!(s.active_worktree.as_deref(), Some("feat-a"));
    }

    #[test]
    fn remove_falls_back_active() {
        let mut s = Store::default();
        s.upsert_worktree(record("feat-a"));
        s.upsert_worktree(record("feat-b"));
        s.set_active("feat-b");
        assert!(s.remove_worktree("feat-b"));
        assert_eq!(s.active_worktree.as_deref(), Some("feat-a"));
    }

    #[test]
    fn set_active_rejects_unknown() {
        let mut s = Store::default();
        assert!(!s.set_active("nope"));
    }
}
