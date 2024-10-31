use crate::collections::Collection;
use crate::paths::{self};
use std::path::Path;
use std::path::PathBuf;

// Represent the category that a card is in, can be nested
#[derive(Ord, PartialOrd, Eq, Hash, Debug, Clone, PartialEq, Default)]
pub struct Category {
    collection: Option<String>,
    dir: Vec<String>,
}

impl Category {
    pub fn join(mut self, s: &str) -> Self {
        self.dir.push(s.to_string());
        self
    }

    pub fn joined(&self) -> String {
        self.dir.join("/")
    }

    pub fn from_dir_path(path: &Path) -> Self {
        // either cards/x/y or collections/colname/x/y
        let path = path
            .strip_prefix(paths::get_share_path())
            .to_owned()
            .unwrap();

        let mut components = path.components();

        let collection = match components.next().unwrap().as_os_str().to_str().unwrap() {
            "cards" => None,
            "collections" => {
                let col_name = components
                    .next()
                    .unwrap()
                    .as_os_str()
                    .to_str()
                    .unwrap()
                    .to_owned();
                Some(col_name)
            }
            _ => panic!(),
        };

        let mut dirs = vec![];

        for c in components {
            let c = c.as_os_str().to_str().unwrap().to_string();
            dirs.push(c);
        }

        let categories = Self {
            collection,
            dir: dirs,
        };

        if categories.as_path().exists() {
            categories
        } else {
            panic!();
        }
    }

    pub fn from_card_path(path: &Path) -> Self {
        let dir = path.parent().unwrap().to_owned();
        Self::from_dir_path(&dir)
    }

    pub fn get_containing_card_paths(&self) -> Vec<PathBuf> {
        let directory = self.as_path();
        let mut paths = vec![];

        for entry in std::fs::read_dir(&directory).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("toml") {
                paths.push(path)
            }
        }
        paths
    }

    pub fn get_following_categories(&self, collection: Option<&Collection>) -> Vec<Self> {
        let categories = Category::load_all(collection);
        let catlen = self.dir.len();
        categories
            .into_iter()
            .filter(|cat| cat.dir.len() >= catlen && cat.dir[0..catlen] == self.dir[0..catlen])
            .collect()
    }

    pub fn print_it(&self) -> String {
        self.dir.last().unwrap_or(&"root".to_string()).clone()
    }

    pub fn print_full(&self) -> String {
        let mut s = "/".to_string();
        s.push_str(&self.joined());
        s
    }

    pub fn print_it_with_depth(&self) -> String {
        let mut s = String::new();
        for _ in 0..self.dir.len() {
            s.push_str("  ");
        }
        format!("{}{}", s, self.print_it())
    }

    fn is_visible_dir(entry: &walkdir::DirEntry) -> bool {
        entry.file_type().is_dir() && !entry.file_name().to_string_lossy().starts_with(".")
    }

    pub fn load_all(collection: Option<&Collection>) -> Vec<Self> {
        let mut output = vec![];
        use walkdir::WalkDir;

        let path = collection
            .map(|col| col.path())
            .unwrap_or_else(|| paths::get_cards_path());

        for entry in WalkDir::new(path)
            .into_iter()
            .filter_entry(|e| Self::is_visible_dir(e))
            .filter_map(Result::ok)
        {
            let cat = Self::from_dir_path(entry.path());
            output.push(cat);
        }

        output
    }

    pub fn as_path(&self) -> PathBuf {
        let categories = self.dir.join("/");
        let prefix = match self.collection.as_ref() {
            Some(col_name) => paths::get_collections_path().join(col_name),
            None => paths::get_cards_path(),
        };

        let path = format!("{}/{}", prefix.display(), categories);
        PathBuf::from(path)
    }
}
