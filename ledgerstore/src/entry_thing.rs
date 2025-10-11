use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use nonempty::NonEmpty;

use crate::{entry_thing::entry::EntryNode, Hashed, LedgerEntry, LedgerEvent, LedgerItem};

pub mod entry {
    use super::*;

    #[derive(Clone, Hash, Debug)]
    pub enum EntryNode<T: LedgerItem> {
        Leaf(LedgerEntry<T>),
        Multiple(Box<NonEmpty<Self>>),
    }

    impl<T: LedgerItem> EntryNode<T> {
        pub fn data_hash(&self) -> Hashed {
            match self {
                EntryNode::Leaf(event) => event.data_hash(),
                EntryNode::Multiple(entries) => entries.last().data_hash(),
            }
        }

        pub fn last_entry(&self) -> &LedgerEntry<T> {
            match self {
                EntryNode::Leaf(event) => event,
                EntryNode::Multiple(list) => list.last().last_entry(),
            }
        }

        fn load_entry(path: &Path) -> Self
        where
            LedgerEvent<T>: serde::de::DeserializeOwned,
        {
            if path.is_dir() {
                let children: Vec<Self> = Self::load_chain(path).into_values().collect();
                let multiple = nonempty::NonEmpty::from_vec(children).unwrap();
                Self::Multiple(Box::new(multiple))
            } else {
                let bytes = std::fs::read(path).unwrap();
                let entry: LedgerEntry<T> = serde_json::from_slice(&bytes).unwrap();
                Self::Leaf(entry)
            }
        }

        /// Read a numeric directory tree into `EntryThing`s.
        ///
        /// - Files become `EntryThing::Leaf`
        /// - Directories become `EntryThing::Multiple`
        pub fn load_chain(root: impl AsRef<Path>) -> BTreeMap<usize, Self>
        where
            LedgerEvent<T>: serde::de::DeserializeOwned,
        {
            let root = root.as_ref();

            let mut entries: Vec<(u64, PathBuf)> = std::fs::read_dir(root)
                .unwrap()
                .filter_map(|res| {
                    let e = res.unwrap();
                    let path = e.path();
                    path.file_name()
                        .and_then(|os| os.to_str())
                        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
                        .and_then(|s| s.parse::<u64>().ok())
                        .map(|n| (n, path))
                })
                .collect();

            entries.sort_by_key(|(n, _)| *n);

            entries
                .into_iter()
                .map(|(idx, path)| (idx as usize, Self::load_entry(&path)))
                .collect()
        }

        /// Format an index as a zero-padded filename (`000123`).
        pub fn index_name(index: usize) -> String {
            format!("{index:06}")
        }

        /// Load a single entry at a given index inside `root`.
        ///
        /// Returns `None` if the file/directory does not exist.
        pub fn load_single(root: impl AsRef<Path>, index: usize) -> Option<Self>
        where
            LedgerEvent<T>: serde::de::DeserializeOwned,
        {
            let root = root.as_ref();
            let path = root.join(Self::index_name(index));
            if path.exists() {
                Some(Self::load_entry(&path))
            } else {
                None
            }
        }
    }

    pub struct EntryNodeIter<T: LedgerItem> {
        stack: Vec<EntryNode<T>>,
    }

    impl<T: LedgerItem> EntryNodeIter<T> {
        fn new(root: EntryNode<T>) -> Self {
            Self { stack: vec![root] }
        }
    }

    impl<T: LedgerItem> Iterator for EntryNodeIter<T> {
        type Item = LedgerEntry<T>;

        fn next(&mut self) -> Option<Self::Item> {
            while let Some(node) = self.stack.pop() {
                match node {
                    EntryNode::Leaf(ev) => return Some(ev),
                    EntryNode::Multiple(children) => {
                        for child in children.into_iter().rev() {
                            self.stack.push(child);
                        }
                    }
                }
            }
            None
        }
    }

    impl<T: LedgerItem> IntoIterator for EntryNode<T> {
        type Item = LedgerEntry<T>;
        type IntoIter = EntryNodeIter<T>;
        fn into_iter(self) -> Self::IntoIter {
            EntryNodeIter::new(self)
        }
    }
}

/// Represents a single entry, which can either be a group of recursive entries or just a singleton.
///
/// Groups are for when logically similar entries. Like if you create and a new object
/// and the creation represents many actions. You'd want to easily undo all of them
/// at the same time.
#[derive(Clone, Hash, Debug)]
pub enum EventNode<T: LedgerItem> {
    Leaf(LedgerEvent<T>),
    Multiple(Box<NonEmpty<Self>>),
}

pub struct EntryIter<T: LedgerItem> {
    stack: Vec<EventNode<T>>,
}

impl<T: LedgerItem> EntryIter<T> {
    fn new(root: EventNode<T>) -> Self {
        Self { stack: vec![root] }
    }
}

impl<T: LedgerItem> Iterator for EntryIter<T> {
    type Item = LedgerEvent<T>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node) = self.stack.pop() {
            match node {
                EventNode::Leaf(ev) => return Some(ev),
                EventNode::Multiple(children) => {
                    for child in children.into_iter().rev() {
                        self.stack.push(child);
                    }
                }
            }
        }
        None
    }
}

impl<T: LedgerItem> IntoIterator for EventNode<T> {
    type Item = LedgerEvent<T>;
    type IntoIter = EntryIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        EntryIter::new(self)
    }
}

impl<T: LedgerItem> EventNode<T> {
    pub fn save(
        self,
        ledger_path: &Path,
        index: usize,
        prev: Option<LedgerEntry<T>>,
    ) -> entry::EntryNode<T> {
        use std::io::Write;

        fn save_entry<T: LedgerItem>(
            dir: &Path,
            index: usize,
            event: LedgerEvent<T>,
            prev: Option<LedgerEntry<T>>,
        ) -> LedgerEntry<T> {
            let entry = LedgerEntry::new(prev.as_ref(), event);
            let name = format!("{index:06}");
            let path = dir.join(name);
            assert!(!path.exists());
            let mut file = std::fs::File::create_new(path).unwrap();
            let serialized = serde_json::to_string_pretty(&entry).unwrap();
            file.write_all(serialized.as_bytes()).unwrap();
            entry
        }

        fn save_entries<T: LedgerItem>(
            dir: &Path,
            index: usize,
            entries: NonEmpty<EventNode<T>>,
            mut prev: Option<LedgerEntry<T>>,
        ) -> EntryNode<T> {
            let mut saved_entries: Vec<EntryNode<T>> = vec![];
            let dir_name = format!("{:06}", index);
            let path = dir.join(dir_name);
            fs::create_dir_all(&path).unwrap();

            for (idx, entry) in entries.into_iter().enumerate() {
                match entry {
                    EventNode::Leaf(event) => {
                        let entry = save_entry(&path, idx, event, prev.clone());
                        prev = Some(entry.clone());
                        saved_entries.push(EntryNode::Leaf(entry));
                    }
                    EventNode::Multiple(entries) => {
                        let entries = save_entries(&path, idx, *entries, prev.clone());
                        prev = Some(entries.last_entry().clone());
                        saved_entries.push(entries);
                    }
                }
            }
            EntryNode::Multiple(Box::new(NonEmpty::from_vec(saved_entries).unwrap()))
        }

        match self {
            EventNode::Leaf(event) => {
                entry::EntryNode::Leaf(save_entry(ledger_path, index, event, prev))
            }

            EventNode::Multiple(entries) => save_entries(ledger_path, index, *entries, prev),
        }
    }

    pub fn new_single(entry: LedgerEvent<T>) -> Self {
        Self::Leaf(entry)
    }

    pub fn new_multiple(entries: NonEmpty<LedgerEvent<T>>) -> Self {
        let multiple: NonEmpty<Self> = entries.map(|entry| Self::new_single(entry));
        Self::Multiple(Box::new(multiple))
    }
}
