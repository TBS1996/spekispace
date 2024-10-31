use std::{
    fmt::{Display, Formatter},
    fs::{self, create_dir_all},
    path::{Path, PathBuf},
};

use git2::{
    Cred, FetchOptions, IndexAddOption, PushOptions, RemoteCallbacks, Repository, Signature,
};

use crate::github::LoginInfo;

pub struct Collection {
    pub name: String,
    pub repo: Repository,
}

impl Display for Collection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl Default for Collection {
    fn default() -> Self {
        Self::load_or_create("personal")
    }
}

pub fn get_dirs(p: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![];

    for entry in fs::read_dir(&p).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        if ty.is_dir() {
            dirs.push(entry.path());
        }
    }

    dirs
}

impl Collection {
    pub fn load_all() -> Vec<Self> {
        let dirs = get_dirs(&get_collections_path());
        let mut names = vec![];
        for dir in &dirs {
            let name = dir.file_name().unwrap().to_str().unwrap();
            names.push(name);
        }

        let mut cols = vec![];

        for name in names {
            if let Some(col) = Self::load(name) {
                cols.push(col);
            }
        }

        if cols.is_empty() {
            cols.push(Self::default());
        }

        cols
    }

    pub fn new(name: String, repo: Repository) -> Self {
        Self { name, repo }
    }

    pub fn set_remote(&self, url: &str) {
        self.repo.remote_set_url("origin", url).unwrap();
        self.repo
            .remote_add_push("origin", "refs/heads/*:refs/remotes/origin/*")
            .unwrap();
    }

    pub fn load(name: &str) -> Option<Self> {
        let path = get_collections_path().join(name);
        if !path.exists() {
            return None;
        }

        let repo = if path.join(".git").exists() {
            Repository::open(&path).unwrap()
        } else {
            Repository::init(path).unwrap()
        };

        Some(Self {
            name: name.to_string(),
            repo,
        })
    }

    pub fn pull(&self) {
        fetch(&self.repo);
        merge(&self.repo);
    }

    pub fn merge_conflict(&self) -> bool {
        self.repo.index().unwrap().has_conflicts()
    }

    pub fn clone(name: &str, remote: &str) -> Self {
        let selv = Self::load_or_create(name);
        selv.set_remote(remote);
        selv
    }

    pub fn create(name: &str) -> Self {
        let path = get_collections_path().join(name);
        create_dir_all(&path).unwrap();
        let repo = Repository::init(path).unwrap();

        Self {
            name: name.to_string(),
            repo,
        }
    }

    pub fn load_or_create(name: &str) -> Self {
        Self::load(name).unwrap_or_else(|| Self::create(name))
    }

    pub fn path(&self) -> PathBuf {
        get_collections_path().join(&self.name)
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub fn get_files(p: &Path) -> Vec<PathBuf> {
    let mut files = vec![];

    for entry in fs::read_dir(&p).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        if ty.is_file() {
            files.push(entry.path());
        }
    }

    files
}

pub fn add(repo: &Repository) {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
}

pub fn commit(repo: &Repository) -> Result<(), git2::Error> {
    // Get the HEAD reference (to find the current commit)
    let parent_commit = {
        let head = repo.head()?;
        repo.find_commit(head.target().unwrap())?
    };

    // Write the tree from the index
    let tree = {
        let tree_oid = repo.index().unwrap().write_tree()?;
        repo.find_tree(tree_oid)?
    };

    let sig = Signature::now("robot", "robot@example.com")?;

    // Create a commit
    repo.commit(Some("HEAD"), &sig, &sig, "save", &tree, &[&parent_commit])?;

    Ok(())
}

pub fn push(repo: &Repository) -> Result<(), git2::Error> {
    let mut remote = repo.find_remote("origin")?;

    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(|_url, _username_from_url, _allowed_types| {
        Cred::userpass_plaintext("oauth2", &LoginInfo::load().unwrap().access_token)
    });

    let mut push_options = PushOptions::new();
    push_options.remote_callbacks(callbacks);

    remote.push(
        &["refs/heads/main:refs/heads/main"],
        Some(&mut push_options),
    )?;

    Ok(())
}

pub fn fetch(repo: &Repository) {
    let mut remote = repo.find_remote("origin").unwrap();
    let mut callbacks = RemoteCallbacks::new();

    callbacks.credentials(|_url, _username_from_url, _allowed_types| {
        Cred::userpass_plaintext("oauth2", &LoginInfo::load().unwrap().access_token)
    });

    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);
    remote
        .fetch(&["refs/heads/main"], Some(&mut fetch_options), None)
        .unwrap();
}

pub fn merge(repo: &Repository) {
    let commit = {
        let fetch_head = repo.find_reference("FETCH_HEAD").unwrap();
        let commit_id = fetch_head.target().unwrap();
        repo.find_commit(commit_id).unwrap()
    };

    let annotated_commit = repo.find_annotated_commit(commit.id()).unwrap();
    let (analysis, _) = repo.merge_analysis(&[&annotated_commit]).unwrap();

    if analysis.is_fast_forward() {
        let refname = "refs/heads/main";
        let mut reference = repo.find_reference(refname).unwrap();
        reference.set_target(commit.id(), "Fast-forward").unwrap();
        repo.set_head(refname).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
            .unwrap();
        println!("Fast-forwarded to latest changes.");
    } else if analysis.is_up_to_date() {
        return;
    } else {
        panic!("Merge required, please resolve manually.");
    }
}
