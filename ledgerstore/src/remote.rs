use chrono::{DateTime, Utc};
use git2::build::CheckoutBuilder;
use git2::{Delta, DiffFindOptions, DiffOptions, ObjectType, Oid, Repository, ResetType};
use indexmap::IndexSet;
use std::path::{Path, PathBuf};

use crate::read_ledger::{FsReadLedger, ReadLedger};
use crate::{DiskDirPath, LedgerItem, PropertyCache};

#[derive(Debug)]
pub struct ChangeSet<T> {
    pub added: Vec<T>,
    pub modified: Vec<T>,
    pub removed: Vec<T>,
}

impl<T> Default for ChangeSet<T> {
    fn default() -> Self {
        Self {
            added: Default::default(),
            modified: Default::default(),
            removed: Default::default(),
        }
    }
}

pub(crate) struct Remote<T: LedgerItem> {
    pub(crate) repo: Repository,
    pub(crate) state: FsReadLedger<T>,
}

impl<T: LedgerItem> ReadLedger for Remote<T> {
    type Item = T;

    fn load(&self, key: <Self::Item as LedgerItem>::Key) -> Option<Self::Item> {
        self.state.load(key)
    }

    fn load_ids(&self) -> IndexSet<<Self::Item as LedgerItem>::Key> {
        self.state.load_ids()
    }

    fn get_property_cache(
        &self,
        cache: PropertyCache<Self::Item>,
    ) -> IndexSet<<Self::Item as LedgerItem>::Key> {
        self.state.get_property_cache(cache)
    }

    fn has_property(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        property: PropertyCache<Self::Item>,
    ) -> bool {
        self.state.has_property(key, property)
    }

    fn get_reference_cache(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        ty: Option<<Self::Item as LedgerItem>::RefType>,
        reversed: bool,
        recursive: bool,
    ) -> IndexSet<<Self::Item as LedgerItem>::Key> {
        self.state.get_reference_cache(key, ty, reversed, recursive)
    }

    fn get_reference_cache_with_ty(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        ty: Option<<Self::Item as LedgerItem>::RefType>,
        reversed: bool,
        recursive: bool,
    ) -> IndexSet<(
        <Self::Item as LedgerItem>::RefType,
        <Self::Item as LedgerItem>::Key,
    )> {
        self.state
            .get_reference_cache_with_ty(key, ty, reversed, recursive)
    }
}

impl<T: LedgerItem> Remote<T> {
    pub fn new(root: &Path) -> Self {
        let path = DiskDirPath::new(root.join("remote")).unwrap();

        let repo = match Repository::open(&*path) {
            Ok(r) => r,
            Err(_) => Repository::init(&*path).unwrap(),
        };

        Self {
            repo,
            state: FsReadLedger::new(path.to_path_buf()),
        }
    }

    pub fn item_path(&self, key: T::Key) -> PathBuf {
        self.state.item_path(key)
    }

    pub fn current_commit(&self) -> Option<String> {
        match self.repo.head() {
            Ok(head) => Some(head.peel_to_commit().unwrap().id().to_string()),
            Err(_) => None,
        }
    }

    pub(crate) fn remote_min_version(
        oid: &str,
        github_user: &str,
        github_repo: &str,
    ) -> Option<semver::Version> {
        let url = format!(
            "https://raw.githubusercontent.com/{github_user}/{github_repo}/{oid}/min_version",
        );

        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(1))
            .build();

        let version: semver::Version = agent
            .get(&url)
            .call()
            .ok()?
            .into_string()
            .unwrap()
            .trim()
            .parse()
            .unwrap();

        Some(version)
    }

    pub fn latest_upstream_commit(&self, url: &str) -> Option<String> {
        let mut r = self.repo.remote_anonymous(url).ok()?;
        r.connect(git2::Direction::Fetch).ok()?;

        let heads = r.list().ok()?;

        let want1 = r
            .default_branch()
            .ok()
            .and_then(|b| b.as_str().map(str::to_owned));

        let want2 = heads.iter().find_map(|h| {
            if h.name() == "HEAD" {
                h.symref_target().map(|s| s.to_string())
            } else {
                None
            }
        });

        let want = want1
            .or(want2)
            .or_else(|| Some("refs/heads/main".to_string()));

        let oid = if let Some(ref want) = want {
            heads.iter().find_map(|h| {
                if h.name() == want.as_str() {
                    Some(h.oid())
                } else {
                    None
                }
            })
        } else {
            None
        }
        .or_else(|| {
            heads.iter().find_map(|h| {
                if h.name() == "refs/heads/master" {
                    Some(h.oid())
                } else {
                    None
                }
            })
        })?;

        Some(oid.to_string())
    }

    pub fn current_commit_date(&self) -> Option<DateTime<Utc>> {
        let commit = self.repo.head().ok()?.peel_to_commit().ok()?;

        let timestamp = commit.time().seconds();
        let naive = DateTime::from_timestamp(timestamp, 0)?;
        Some(naive)
    }

    pub fn changed_paths(&self, old_commit: Option<&str>, new_commit: &str) -> ChangeSet<PathBuf> {
        // resolve new
        let new_oid = Oid::from_str(new_commit).unwrap();
        let new_commit = self
            .repo
            .find_object(new_oid, Some(ObjectType::Commit))
            .unwrap()
            .into_commit()
            .unwrap();
        let new_tree = new_commit.tree().unwrap();

        let old_tree = old_commit.map(|s| {
            let old_oid = Oid::from_str(s).unwrap();
            let old_commit = self
                .repo
                .find_object(old_oid, Some(ObjectType::Commit))
                .unwrap()
                .into_commit()
                .unwrap();
            old_commit.tree().unwrap()
        });

        let mut opts = DiffOptions::new();
        let mut diff = self
            .repo
            .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut opts))
            .unwrap();

        let mut find = DiffFindOptions::new();
        find.renames(true).renames_from_rewrites(true);
        diff.find_similar(Some(&mut find)).unwrap();

        let mut out = ChangeSet {
            added: vec![],
            modified: vec![],
            removed: vec![],
        };

        diff.foreach(
            &mut |d, _| {
                match d.status() {
                    Delta::Added | Delta::Copied => {
                        let p = d.new_file().path().unwrap().to_path_buf();
                        out.added.push(p);
                    }
                    Delta::Modified | Delta::Typechange => {
                        let p = d.new_file().path().unwrap().to_path_buf();
                        out.modified.push(p);
                    }
                    Delta::Deleted => {
                        let p = d.old_file().path().unwrap().to_path_buf();
                        out.removed.push(p);
                    }
                    Delta::Renamed => {
                        let oldp = d.old_file().path().unwrap().to_path_buf();
                        let newp = d.new_file().path().unwrap().to_path_buf();
                        out.removed.push(oldp);
                        out.added.push(newp);
                    }
                    // Untracked/Unmodified/Ignored/Conflicted → skip
                    _ => {}
                }
                true
            },
            None,
            None,
            None,
        )
        .unwrap();

        out
    }

    pub(crate) fn modified_items(
        &self,
        old_commit: Option<&str>,
        new_commit: &str,
    ) -> ChangeSet<T::Key> {
        let mut changed_items: ChangeSet<T::Key> = Default::default();

        let changed_paths = self.changed_paths(old_commit, new_commit);

        for path in changed_paths.added {
            if path.starts_with("items") {
                let filename = path.file_name().unwrap().to_str().unwrap();
                match filename.parse() {
                    Ok(key) => {
                        changed_items.added.push(key);
                    }
                    Err(_) => {
                        dbg!(&filename);
                        panic!();
                    }
                }
            }
        }

        for path in changed_paths.modified {
            if path.starts_with("items") {
                let filename = path.file_name().unwrap().to_str().unwrap();
                match filename.parse() {
                    Ok(key) => {
                        changed_items.modified.push(key);
                    }
                    Err(_) => {
                        dbg!(&filename);
                        panic!();
                    }
                }
            }
        }
        for path in changed_paths.removed {
            if path.starts_with("items") {
                let filename = path.file_name().unwrap().to_str().unwrap();
                match filename.parse() {
                    Ok(key) => {
                        changed_items.removed.push(key);
                    }
                    Err(_) => {
                        dbg!(&filename);
                        panic!();
                    }
                }
            }
        }

        changed_items
    }

    #[allow(unused)]
    pub fn hard_reset_current(&self) -> Result<(), git2::Error> {
        let head = self.repo.head()?;
        let obj = head.peel(git2::ObjectType::Commit)?;

        let mut cb = CheckoutBuilder::new();
        cb.force()
            .remove_untracked(true)
            .remove_ignored(true)
            .recreate_missing(true);

        self.repo.checkout_tree(&obj, Some(&mut cb))?;
        self.repo.reset(&obj, ResetType::Hard, Some(&mut cb))?;

        let _ = self.repo.cleanup_state();
        Ok(())
    }

    pub fn checkout_empty(&self) -> Result<(), git2::Error> {
        let mut index = self.repo.index().unwrap();
        index.clear().unwrap();
        index.write().unwrap(); // save empty index

        let mut opts = CheckoutBuilder::new();
        opts.force().remove_untracked(true).remove_ignored(true);
        self.repo
            .checkout_index(Some(&mut index), Some(&mut opts))
            .unwrap();

        Ok(())
    }

    pub fn reset_to_empty(&self) {
        let refs = self.repo.references().unwrap();
        for r in refs {
            let r = r.unwrap();
            if let Some(name) = r.name() {
                if name.starts_with("refs/heads/") || name.starts_with("refs/tags/") {
                    self.repo.find_reference(name).unwrap().delete().unwrap();
                }
            }
        }

        self.repo.set_head("refs/heads/main").unwrap();

        let mut cb = git2::build::CheckoutBuilder::new();
        cb.force().remove_untracked(true).remove_ignored(true);
        self.repo.checkout_head(Some(&mut cb)).unwrap();
    }

    pub fn set_commit_clean(
        &self,
        upstream_url: Option<&str>,
        commit: &str,
    ) -> Result<ChangeSet<T::Key>, git2::Error> {
        let curr_commit = self.current_commit();

        if let Some(upstream_url) = upstream_url {
            self.fetch_heads_anonymous(upstream_url)?;
        }

        let oid = Oid::from_str(commit)?;
        let _obj = self.repo.find_object(oid, None)?;

        self.repo.set_head_detached(oid)?;
        let obj = self.repo.find_object(oid, None)?;
        self.repo.reset(&obj, ResetType::Hard, None)?;

        debug_assert_eq!(self.current_commit().as_deref(), Some(commit));

        let diffs = self.modified_items(curr_commit.as_deref(), commit);
        Ok(diffs)
    }

    fn fetch_heads_anonymous(&self, url: &str) -> Result<(), git2::Error> {
        use git2::AutotagOption;
        use git2::FetchOptions;

        let mut remote = self.repo.remote_anonymous(url)?;

        let mut opts = FetchOptions::new();
        opts.download_tags(AutotagOption::None);

        let refspecs = ["+refs/heads/*:refs/remotes/__tmp/*"];

        remote.fetch(&refspecs, Some(&mut opts), None)
    }
}
