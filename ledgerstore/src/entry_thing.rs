use std::{
    collections::BTreeMap,
    fs,
    ops::Deref,
    path::{Path, PathBuf},
};

use nonempty::NonEmpty;

use crate::{
    node::{Node, NodeIterRef},
    Hashed, LedgerEntry, LedgerEvent, LedgerItem,
};

#[derive(Clone, Hash, Debug)]
pub struct EntryNode<T: LedgerItem>(Node<LedgerEntry<T>>);

impl<T: LedgerItem> Deref for EntryNode<T> {
    type Target = Node<LedgerEntry<T>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// Borrowed iteration: `for e in &entry_node { ... }`
impl<'a, T: LedgerItem> IntoIterator for &'a EntryNode<T> {
    type Item = &'a LedgerEntry<T>;
    type IntoIter = NodeIterRef<'a, LedgerEntry<T>>;
    fn into_iter(self) -> Self::IntoIter {
        (&self.0).into_iter()
    }
}

impl<T: LedgerItem> EntryNode<T> {
    pub fn data_hash(&self) -> Hashed {
        self.0.last().data_hash()
    }

    fn load_entry(path: &Path) -> Self
    where
        LedgerEvent<T>: serde::de::DeserializeOwned,
    {
        if path.is_dir() {
            let children: Vec<Self> = Self::load_chain(path).into_values().collect();
            let multiple = nonempty::NonEmpty::from_vec(children).unwrap();
            let node_multiple = multiple.map(|entry_node| entry_node.0.clone());
            Self(Node::Branch(Box::new(node_multiple)))
        } else {
            let bytes = std::fs::read(path).unwrap();
            let entry: LedgerEntry<T> = serde_json::from_slice(&bytes).unwrap();
            Self(Node::Leaf(entry))
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

#[derive(Clone, Hash, Debug)]
pub struct EventNode<T: LedgerItem>(Node<LedgerEvent<T>>);

impl<T: LedgerItem> Deref for EventNode<T> {
    type Target = Node<LedgerEvent<T>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: LedgerItem> EventNode<T> {
    pub fn save(
        self,
        ledger_path: &Path,
        index: usize,
        prev: Option<LedgerEntry<T>>,
    ) -> EntryNode<T> {
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
            entries: NonEmpty<Node<LedgerEvent<T>>>,
            mut prev: Option<LedgerEntry<T>>,
        ) -> EntryNode<T> {
            let mut saved_entries: Vec<EntryNode<T>> = vec![];
            let dir_name = format!("{:06}", index);
            let path = dir.join(dir_name);
            fs::create_dir_all(&path).unwrap();

            for (idx, entry) in entries.into_iter().enumerate() {
                match entry {
                    Node::Leaf(event) => {
                        let entry = save_entry(&path, idx, event, prev.clone());
                        prev = Some(entry.clone());
                        saved_entries.push(EntryNode(Node::Leaf(entry)));
                    }
                    Node::Branch(entries) => {
                        let entries = save_entries(&path, idx, *entries, prev.clone());
                        prev = Some(entries.last().clone());
                        saved_entries.push(entries);
                    }
                }
            }
            EntryNode(Node::Branch(Box::new(
                NonEmpty::from_vec(
                    saved_entries
                        .into_iter()
                        .map(|entry_node| entry_node.0)
                        .collect(),
                )
                .unwrap(),
            )))
        }

        match self {
            EventNode(Node::Leaf(event)) => {
                EntryNode(Node::Leaf(save_entry(ledger_path, index, event, prev)))
            }

            EventNode(Node::Branch(entries)) => save_entries(ledger_path, index, *entries, prev),
        }
    }

    pub fn new_leaf(entry: LedgerEvent<T>) -> Self {
        Self(Node::new_leaf(entry))
    }

    pub fn new_branch(entries: NonEmpty<LedgerEvent<T>>) -> Self {
        let multiple: NonEmpty<Node<LedgerEvent<T>>> = entries.map(|entry| Node::new_leaf(entry));
        Self(Node::new_branch(multiple))
    }
}
