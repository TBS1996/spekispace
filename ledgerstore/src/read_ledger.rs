use std::collections::HashSet;
use std::fs::{self};
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;
use std::vec::Vec;
use walkdir::WalkDir;

use crate::{DiskDirPath, ItemExpr, LedgerItem, Node, PropertyCache};

/// Core trait with minimal required methods - everything else composes from these
pub trait ReadLedger {
    type Item: LedgerItem;

    /// Load a single item by its key
    fn load(&self, key: <Self::Item as LedgerItem>::Key) -> Option<Self::Item>;

    /// Get all item keys that exist in the ledger
    fn load_ids(&self) -> HashSet<<Self::Item as LedgerItem>::Key>;

    /// Get all items that have a specific property value
    fn get_property_cache(
        &self,
        cache: PropertyCache<Self::Item>,
    ) -> HashSet<<Self::Item as LedgerItem>::Key>;

    /// Get items related via references
    /// - reversed=false: items that `key` depends on (dependencies)
    /// - reversed=true: items that depend on `key` (dependents)
    /// - ty=None: all reference types
    /// - ty=Some(t): only references of type `t`
    /// - recursive=true: transitively follow references
    fn get_reference_cache(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        ty: Option<<Self::Item as LedgerItem>::RefType>,
        reversed: bool,
        recursive: bool,
    ) -> HashSet<<Self::Item as LedgerItem>::Key>;

    /// Same as get_reference_cache but returns (RefType, Key) tuples
    fn get_reference_cache_with_ty(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        ty: Option<<Self::Item as LedgerItem>::RefType>,
        reversed: bool,
        recursive: bool,
    ) -> HashSet<(
        <Self::Item as LedgerItem>::RefType,
        <Self::Item as LedgerItem>::Key,
    )>;

    // Provided methods that compose from the primitives above:

    fn load_all(&self) -> HashSet<Self::Item> {
        let ids = self.load_ids();
        let mut out = HashSet::with_capacity(ids.len());

        for id in ids {
            if let Some(item) = self.load(id) {
                out.insert(item);
            }
        }

        out
    }

    fn has_item(&self, key: <Self::Item as LedgerItem>::Key) -> bool {
        self.load(key).is_some()
    }

    fn has_property(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        property: PropertyCache<Self::Item>,
    ) -> bool {
        self.get_property_cache(property).contains(&key)
    }

    #[allow(dead_code)]
    fn direct_dependencies(
        &self,
        key: <Self::Item as LedgerItem>::Key,
    ) -> HashSet<<Self::Item as LedgerItem>::Key> {
        self.get_reference_cache(key, None, false, false)
    }

    fn recursive_dependencies(
        &self,
        key: <Self::Item as LedgerItem>::Key,
    ) -> HashSet<<Self::Item as LedgerItem>::Key> {
        self.get_reference_cache(key, None, false, true)
    }

    fn direct_dependents(
        &self,
        key: <Self::Item as LedgerItem>::Key,
    ) -> HashSet<<Self::Item as LedgerItem>::Key> {
        self.get_reference_cache(key, None, true, false)
    }

    fn recursive_dependents(
        &self,
        key: <Self::Item as LedgerItem>::Key,
    ) -> HashSet<<Self::Item as LedgerItem>::Key> {
        self.get_reference_cache(key, None, true, true)
    }

    fn all_dependents_with_ty(
        &self,
        key: <Self::Item as LedgerItem>::Key,
    ) -> HashSet<(
        <Self::Item as LedgerItem>::RefType,
        <Self::Item as LedgerItem>::Key,
    )> {
        self.get_reference_cache_with_ty(key, None, true, true)
    }

    fn collect_all_dependents_recursive_struct(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        reversed: bool,
    ) -> Node<Self::Item> {
        let mut node: Node<Self::Item> = Node {
            id: key,
            deps: vec![],
        };

        // Get direct references (not recursive)
        let refs = self.get_reference_cache(key, None, reversed, false);

        for ref_key in refs {
            let child_node = self.collect_all_dependents_recursive_struct(ref_key, reversed);
            node.deps.push(child_node);
        }

        node
    }

    fn load_expr(&self, set: ItemExpr<Self::Item>) -> HashSet<<Self::Item as LedgerItem>::Key> {
        match set {
            ItemExpr::Union(nodes) => nodes.into_iter().flat_map(|n| self.load_expr(n)).collect(),
            ItemExpr::Intersection(nodes) => {
                let mut iter = nodes.into_iter();

                let mut out = match iter.next() {
                    Some(items) => self.load_expr(items),
                    None => return Default::default(),
                };

                for node in iter {
                    out = out.intersection(&self.load_expr(node)).cloned().collect();
                }

                out
            }
            ItemExpr::Difference(expr1, expr2) => {
                let keys_1 = self.load_expr(*expr1);
                let keys_2 = self.load_expr(*expr2);
                keys_1.difference(&keys_2).cloned().collect()
            }
            ItemExpr::Complement(expr) => self
                .load_ids()
                .difference(&self.load_expr(*expr))
                .cloned()
                .collect(),
            ItemExpr::All => self.load_ids(),
            ItemExpr::Item(key) => [key].into_iter().collect(),
            ItemExpr::Property { property, value } => {
                self.get_property_cache(PropertyCache { property, value })
            }
            ItemExpr::Reference {
                items,
                ty,
                reversed,
                recursive,
                include_self,
            } => {
                let mut out = HashSet::new();
                let items = self.load_expr(*items);

                for item in items {
                    let refs = self.get_reference_cache(item, ty.clone(), reversed, recursive);
                    out.extend(refs);

                    if include_self {
                        out.insert(item);
                    }
                }

                out
            }
        }
    }
}

/// Filesystem-based implementation of ReadLedger
#[derive(Clone, Debug)]
pub struct FsReadLedger<T: LedgerItem> {
    root: PathBuf,
    properties: Arc<DiskDirPath>,
    dependencies: Arc<DiskDirPath>,
    dependents: Arc<DiskDirPath>,
    items: Arc<DiskDirPath>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: LedgerItem> FsReadLedger<T> {
    pub fn new(root: PathBuf) -> Self {
        Self {
            properties: Arc::new(DiskDirPath::new(root.join("properties")).unwrap()),
            dependencies: Arc::new(DiskDirPath::new(root.join("dependencies")).unwrap()),
            dependents: Arc::new(DiskDirPath::new(root.join("dependents")).unwrap()),
            items: Arc::new(DiskDirPath::new(root.join("items")).unwrap()),
            root,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn clear_state(&self) {
        self.properties.clear_contents().unwrap();
        self.dependencies.clear_contents().unwrap();
        self.dependents.clear_contents().unwrap();
        self.items.clear_contents().unwrap();
    }

    fn items_path(&self) -> DiskDirPath {
        let p = self.root.join("items");
        DiskDirPath::new(p).unwrap()
    }

    fn dependents_path(&self) -> DiskDirPath {
        let p = self.root.join("dependents");
        DiskDirPath::new(p).unwrap()
    }
    fn dependencies_path(&self) -> DiskDirPath {
        let p = self.root.join("dependencies");
        DiskDirPath::new(p).unwrap()
    }

    pub fn properties_path(&self) -> Arc<DiskDirPath> {
        self.properties.clone()
    }

    pub fn root_dependencies_dir(&self, key: T::Key) -> PathBuf {
        self.dependencies_path().join(key.to_string())
    }

    pub fn root_dependents_dir(&self, key: T::Key) -> PathBuf {
        self.dependents_path().join(key.to_string())
    }

    pub fn has_property(&self, key: T::Key, property: PropertyCache<T>) -> bool {
        self.properties_path()
            .join(property.property.to_string())
            .join(&property.value)
            .join(key.to_string())
            .is_file()
    }

    pub fn item_path(&self, key: T::Key) -> PathBuf {
        let key_str = key.to_string();
        let mut chars = key_str.chars();

        let prefix = if let (Some(ch1), Some(ch2)) = (chars.next(), chars.next()) {
            format!("{}{}", ch1, ch2)
        } else {
            panic!("Key too short");
        };

        let prefix = self.items_path().join(prefix);

        prefix.join(key_str)
    }

    pub fn item_path_create(&self, key: T::Key) -> PathBuf {
        let key_str = key.to_string();
        let mut chars = key_str.chars();

        let prefix = if let (Some(ch1), Some(ch2)) = (chars.next(), chars.next()) {
            format!("{}{}", ch1, ch2)
        } else {
            panic!("Key too short");
        };

        let prefix = self.items_path().join(prefix);
        fs::create_dir_all(&prefix).unwrap();

        prefix.join(key_str)
    }

    fn item_keys_from_dir(path: PathBuf) -> HashSet<T::Key> {
        if !path.exists() {
            Default::default()
        } else {
            let mut out = HashSet::default();
            for entry in fs::read_dir(&path).unwrap() {
                match entry
                    .unwrap()
                    .file_name()
                    .to_str()
                    .unwrap()
                    .parse::<T::Key>()
                {
                    Ok(key) => out.insert(key),
                    Err(_e) => {
                        dbg!(path);
                        panic!();
                    }
                };
            }
            out
        }
    }

    fn item_keys_from_dir_recursive(path: PathBuf) -> HashSet<T::Key> {
        if !path.exists() {
            return Default::default();
        }

        let mut out = HashSet::new();

        for entry in WalkDir::new(&path)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            match entry.file_name().to_str().unwrap().parse::<T::Key>() {
                Ok(key) => {
                    out.insert(key);
                }
                Err(_) => {
                    dbg!(entry.path());
                    panic!("Failed to parse key from file name");
                }
            }
        }

        out
    }

    fn collect_references_recursive(
        &self,
        key: T::Key,
        ty: Option<T::RefType>,
        out: &mut HashSet<T::Key>,
        reversed: bool,
    ) {
        let dep_dir = match reversed {
            true => self.root_dependents_dir(key),
            false => self.root_dependencies_dir(key),
        };

        if !dep_dir.exists() {
            return;
        }

        let dirs = match ty.clone() {
            Some(ty) => vec![dep_dir.join(ty.to_string())],
            None => fs::read_dir(&dep_dir)
                .unwrap()
                .filter_map(|entry| {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        Some(path)
                    } else {
                        None
                    }
                })
                .collect(),
        };

        for dir in dirs {
            for dep_key in Self::item_keys_from_dir(dir) {
                if out.insert(dep_key.clone()) {
                    self.collect_references_recursive(dep_key, ty.clone(), out, reversed);
                }
            }
        }
    }

    fn collect_references_recursive_with_ty(
        &self,
        key: T::Key,
        ty: Option<T::RefType>,
        out: &mut HashSet<(T::RefType, T::Key)>,
        reversed: bool,
    ) {
        let dep_dir = match reversed {
            true => self.root_dependents_dir(key),
            false => self.root_dependencies_dir(key),
        };

        if !dep_dir.exists() {
            return;
        }

        let dirs = match ty.clone() {
            Some(ty) => vec![dep_dir.join(ty.to_string())],
            None => fs::read_dir(&dep_dir)
                .unwrap()
                .filter_map(|entry| {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        Some(path)
                    } else {
                        None
                    }
                })
                .collect(),
        };

        for dir in dirs {
            let the_ty: T::RefType = match dir.file_name().unwrap().to_str().unwrap().parse() {
                Ok(ty) => ty,
                Err(_) => panic!(),
            };

            for dep_key in Self::item_keys_from_dir(dir) {
                if out.insert((the_ty.clone(), dep_key.clone())) {
                    self.collect_references_recursive_with_ty(dep_key, ty.clone(), out, reversed);
                }
            }
        }
    }
}

impl<T: LedgerItem> ReadLedger for FsReadLedger<T> {
    type Item = T;

    fn load(&self, key: T::Key) -> Option<T> {
        let path = self.item_path(key);

        if path.is_file() {
            let s = fs::read_to_string(&path);
            match s {
                Ok(s) => match serde_json::from_str(&s) {
                    Ok(val) => Some(val),
                    Err(e) => {
                        dbg!(s);
                        dbg!(e);
                        dbg!(key);
                        dbg!(path);
                        panic!();
                    }
                },
                Err(e) => {
                    dbg!(e);
                    dbg!(key);
                    dbg!(path);
                    panic!();
                }
            }
        } else {
            None
        }
    }

    fn load_ids(&self) -> HashSet<T::Key> {
        let mut entries: Vec<PathBuf> = vec![];

        for entry in fs::read_dir(&*self.items_path()).unwrap() {
            let entry = entry.unwrap().path();
            entries.push(entry);
        }
        let mut keys: HashSet<T::Key> = HashSet::default();

        for entry in entries {
            for entry in fs::read_dir(entry).unwrap() {
                match entry
                    .unwrap()
                    .file_name()
                    .to_str()
                    .unwrap()
                    .parse::<T::Key>()
                {
                    Ok(key) => keys.insert(key),
                    Err(_) => panic!(),
                };
            }
        }

        keys
    }

    fn get_property_cache(&self, cache: PropertyCache<T>) -> HashSet<T::Key> {
        let path = self
            .properties_path()
            .join(cache.property.to_string())
            .join(&cache.value);
        Self::item_keys_from_dir(path)
    }

    fn has_property(&self, key: T::Key, property: PropertyCache<T>) -> bool {
        self.properties_path()
            .join(property.property.to_string())
            .join(&property.value)
            .join(key.to_string())
            .is_file()
    }

    fn get_reference_cache(
        &self,
        key: T::Key,
        ty: Option<T::RefType>,
        reversed: bool,
        recursive: bool,
    ) -> HashSet<T::Key> {
        if recursive {
            let mut out = HashSet::new();
            self.collect_references_recursive(key, ty, &mut out, reversed);
            out
        } else {
            let dir = match reversed {
                true => match ty {
                    Some(ty) => self.root_dependents_dir(key).join(ty.to_string()),
                    None => {
                        return Self::item_keys_from_dir_recursive(self.root_dependents_dir(key))
                    }
                },
                false => match ty {
                    Some(ty) => self.root_dependencies_dir(key).join(ty.to_string()),
                    None => {
                        return Self::item_keys_from_dir_recursive(self.root_dependencies_dir(key))
                    }
                },
            };
            Self::item_keys_from_dir(dir)
        }
    }

    fn get_reference_cache_with_ty(
        &self,
        key: T::Key,
        ty: Option<T::RefType>,
        reversed: bool,
        recursive: bool,
    ) -> HashSet<(T::RefType, T::Key)> {
        if recursive {
            let mut out = HashSet::new();
            self.collect_references_recursive_with_ty(key, ty, &mut out, reversed);
            out
        } else {
            let mut out: HashSet<(T::RefType, T::Key)> = Default::default();
            let dep_dir = match reversed {
                true => self.root_dependents_dir(key),
                false => self.root_dependencies_dir(key),
            };

            let topdir = match fs::read_dir(&dep_dir) {
                Ok(dir) => dir,
                Err(e) if e.kind() == ErrorKind::NotFound => return Default::default(),
                Err(e) => {
                    dbg!(key, dep_dir, e);
                    panic!();
                }
            };

            for dir in topdir {
                let dir = dir.unwrap();
                let the_ty: T::RefType = match dir.file_name().to_str().unwrap().parse() {
                    Ok(ty) => ty,
                    Err(_) => panic!(),
                };

                if let Some(ref filter_ty) = ty {
                    if &the_ty != filter_ty {
                        continue;
                    }
                }

                for dep_key in Self::item_keys_from_dir(dir.path()) {
                    out.insert((the_ty.clone(), dep_key));
                }
            }

            out
        }
    }
}
