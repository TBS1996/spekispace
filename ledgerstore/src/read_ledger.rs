use either::Either;
use std::collections::HashSet;
use std::fs::{self};
use std::path::PathBuf;
use std::vec::Vec;
use walkdir::WalkDir;

pub type CacheKey<T> = Either<PropertyCache<T>, ItemRefCache<T>>;
use crate::{ItemRefCache, LedgerItem, PropertyCache, RefGetter, TheCacheGetter};

pub trait ReadLedger {
    type Item: LedgerItem;

    fn root_path(&self) -> PathBuf;

    fn load_all(&self) -> HashSet<Self::Item> {
        let ids = self.load_ids();
        let mut out = HashSet::with_capacity(ids.len());

        for id in ids {
            out.insert(self.load(id).unwrap());
        }

        out
    }

    fn load_ids(&self) -> HashSet<<Self::Item as LedgerItem>::Key> {
        let mut entries: Vec<PathBuf> = vec![];

        for entry in fs::read_dir(self.items_path()).unwrap() {
            let entry = entry.unwrap().path();
            entries.push(entry);
        }
        let mut keys: HashSet<<Self::Item as LedgerItem>::Key> = HashSet::default();

        for entry in entries {
            for entry in fs::read_dir(entry).unwrap() {
                match entry
                    .unwrap()
                    .file_name()
                    .to_str()
                    .unwrap()
                    .parse::<<Self::Item as LedgerItem>::Key>()
                {
                    Ok(key) => keys.insert(key),
                    Err(_) => panic!(),
                };
            }
        }

        keys
    }

    fn cache_dir(&self, cache: CacheKey<Self::Item>) -> PathBuf {
        match cache {
            CacheKey::Left(PropertyCache { property, value }) => self
                .properties_path()
                .join(property.to_string())
                .join(&value),
            CacheKey::Right(ItemRefCache { reftype, id }) => {
                self.root_dependents_dir(id).join(reftype.to_string())
            }
        }
    }

    fn properties_path(&self) -> PathBuf {
        let p = self.root_path().join("properties");
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn get_prop_cache(
        &self,
        key: PropertyCache<Self::Item>,
    ) -> HashSet<<Self::Item as LedgerItem>::Key> {
        let path = self
            .properties_path()
            .join(key.property.to_string())
            .join(&key.value);
        Self::item_keys_from_dir(path)
    }

    fn all_dependencies(
        &self,
        key: <Self::Item as LedgerItem>::Key,
    ) -> HashSet<<Self::Item as LedgerItem>::Key> {
        let getter = RefGetter {
            reversed: false,
            key,
            ty: None,
            recursive: false,
        };

        self.load_getter_ty(getter)
            .into_iter()
            .map(|x| x.1)
            .collect()
    }

    fn all_dependents_with_ty(
        &self,
        key: <Self::Item as LedgerItem>::Key,
    ) -> HashSet<(
        <Self::Item as LedgerItem>::RefType,
        <Self::Item as LedgerItem>::Key,
    )> {
        let getter = RefGetter {
            reversed: true,
            key,
            ty: None,
            recursive: true,
        };
        self.load_getter_ty(getter)
    }

    fn dependents(
        &self,
        key: <Self::Item as LedgerItem>::Key,
    ) -> HashSet<<Self::Item as LedgerItem>::Key> {
        let getter = RefGetter {
            reversed: true,
            key,
            ty: None,
            recursive: false,
        };
        self.load_getter_ty(getter)
            .into_iter()
            .map(|x| x.1)
            .collect()
    }

    fn all_dependents(
        &self,
        key: <Self::Item as LedgerItem>::Key,
    ) -> HashSet<<Self::Item as LedgerItem>::Key> {
        self.all_dependents_with_ty(key)
            .into_iter()
            .map(|x| x.1)
            .collect()
    }

    fn item_keys_from_dir_recursive(path: PathBuf) -> HashSet<<Self::Item as LedgerItem>::Key> {
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
            match entry
                .file_name()
                .to_str()
                .unwrap()
                .parse::<<Self::Item as LedgerItem>::Key>()
            {
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

    fn load_getter(
        &self,
        getter: TheCacheGetter<Self::Item>,
    ) -> HashSet<<Self::Item as LedgerItem>::Key> {
        match getter {
            TheCacheGetter::ItemRef(RefGetter {
                recursive: true,
                reversed,
                key,
                ty,
            }) => {
                let mut out = HashSet::new();
                self.collect_all_dependents_recursive(key, ty, &mut out, reversed);
                out
            }
            TheCacheGetter::ItemRef(RefGetter {
                recursive: false,
                reversed: true,
                key,
                ty: Some(ty),
            }) => {
                let dir = self.dependents_dir(key, ty);
                Self::item_keys_from_dir(dir)
            }
            TheCacheGetter::ItemRef(RefGetter {
                recursive: false,
                reversed: true,
                key,
                ty: None,
            }) => {
                let dep_dir = self.root_dependents_dir(key);
                Self::item_keys_from_dir_recursive(dep_dir)
            }
            TheCacheGetter::ItemRef(RefGetter {
                recursive: false,
                reversed: false,
                key,
                ty: Some(ty),
            }) => {
                let dir = self.root_dependencies_dir(key).join(ty.to_string());
                Self::item_keys_from_dir(dir)
            }
            TheCacheGetter::ItemRef(RefGetter {
                recursive: false,
                reversed: false,
                key,
                ty: None,
            }) => {
                let dir = self.root_dependencies_dir(key);
                Self::item_keys_from_dir_recursive(dir)
            }
            TheCacheGetter::Property(prop) => self.get_prop_cache(prop),
        }
    }

    fn root_dependencies_dir(&self, key: <Self::Item as LedgerItem>::Key) -> PathBuf {
        let p = self.root_path().join("dependencies").join(key.to_string());
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn root_dependents_dir(&self, key: <Self::Item as LedgerItem>::Key) -> PathBuf {
        let p = self.root_path().join("dependents").join(key.to_string());
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn dependents_dir(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        ty: <Self::Item as LedgerItem>::RefType,
    ) -> PathBuf {
        let p = self.root_dependents_dir(key).join(ty.to_string());
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn items_path(&self) -> PathBuf {
        let p = self.root_path().join("items");
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn collect_all_dependents_recursive(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        ty: Option<<Self::Item as LedgerItem>::RefType>,
        out: &mut HashSet<<Self::Item as LedgerItem>::Key>,
        reversed: bool,
    ) {
        let dep_dir = match reversed {
            true => self.root_dependents_dir(key),
            false => self.root_dependencies_dir(key),
        };

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
                    self.collect_all_dependents_recursive(dep_key, ty.clone(), out, reversed);
                }
            }
        }
    }

    fn collect_all_dependents_recursive_with_ty(
        &self,
        key: <Self::Item as LedgerItem>::Key,
        ty: Option<<Self::Item as LedgerItem>::RefType>,
        out: &mut HashSet<(
            <Self::Item as LedgerItem>::RefType,
            <Self::Item as LedgerItem>::Key,
        )>,
        reversed: bool,
    ) {
        let dep_dir = match reversed {
            true => self.root_dependents_dir(key),
            false => self.root_dependencies_dir(key),
        };

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
            let the_ty: <Self::Item as LedgerItem>::RefType =
                match dir.file_name().unwrap().to_str().unwrap().parse() {
                    Ok(ty) => ty,
                    Err(_) => panic!(),
                };

            for dep_key in Self::item_keys_from_dir(dir) {
                if out.insert((the_ty.clone(), dep_key.clone())) {
                    self.collect_all_dependents_recursive_with_ty(
                        dep_key,
                        ty.clone(),
                        out,
                        reversed,
                    );
                }
            }
        }
    }

    /// Returns the path a key corresponds to. Does not guarantee that the item is actually there.
    fn item_path(&self, key: <Self::Item as LedgerItem>::Key) -> PathBuf {
        let key = key.to_string();
        let mut chars = key.chars();

        let prefix = if let (Some(ch1), Some(ch2)) = (chars.next(), chars.next()) {
            format!("{}{}", ch1, ch2)
        } else {
            panic!();
        };

        let prefix = self.items_path().join(prefix);
        fs::create_dir_all(&prefix).unwrap();

        prefix.join(key)
    }

    fn item_keys_from_dir(path: PathBuf) -> HashSet<<Self::Item as LedgerItem>::Key> {
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
                    .parse::<<Self::Item as LedgerItem>::Key>()
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

    fn load_getter_ty(
        &self,
        getter: RefGetter<Self::Item>,
    ) -> HashSet<(
        <Self::Item as LedgerItem>::RefType,
        <Self::Item as LedgerItem>::Key,
    )> {
        let RefGetter {
            reversed,
            key,
            ty,
            recursive,
        } = getter;

        match (recursive, reversed, key, ty) {
            (true, reversed, key, ty) => {
                let mut out = HashSet::new();
                self.collect_all_dependents_recursive_with_ty(key, ty, &mut out, reversed);
                out
            }
            (false, true, key, Some(ty)) => {
                let dir = self.dependents_dir(key, ty.clone());
                Self::item_keys_from_dir(dir)
                    .into_iter()
                    .map(|key| (ty.clone(), key))
                    .collect()
            }
            (false, true, key, None) => {
                let mut out: HashSet<(
                    <Self::Item as LedgerItem>::RefType,
                    <Self::Item as LedgerItem>::Key,
                )> = Default::default();
                let dep_dir = self.root_dependents_dir(key);
                for dir in fs::read_dir(&dep_dir).unwrap() {
                    let dir = dir.unwrap();
                    let ty: <Self::Item as LedgerItem>::RefType =
                        match dir.file_name().to_str().unwrap().parse() {
                            Ok(ty) => ty,
                            Err(_) => panic!(),
                        };

                    for key in Self::item_keys_from_dir(dir.path()) {
                        out.insert((ty.clone(), key));
                    }
                }

                out
            }
            (false, false, key, Some(ty)) => {
                let dir = self.root_dependencies_dir(key).join(ty.to_string());
                Self::item_keys_from_dir(dir)
                    .into_iter()
                    .map(|key| (ty.clone(), key))
                    .collect()
            }
            (false, false, key, None) => {
                let mut out: HashSet<(
                    <Self::Item as LedgerItem>::RefType,
                    <Self::Item as LedgerItem>::Key,
                )> = Default::default();
                let dep_dir = self.root_dependencies_dir(key);
                for dir in fs::read_dir(&dep_dir).unwrap() {
                    let dir = dir.unwrap();
                    let ty: <Self::Item as LedgerItem>::RefType =
                        match dir.file_name().to_str().unwrap().parse() {
                            Ok(ty) => ty,
                            Err(_) => panic!(),
                        };

                    for key in Self::item_keys_from_dir(dir.path()) {
                        out.insert((ty.clone(), key));
                    }
                }

                out
            }
        }
    }

    fn load(&self, key: <Self::Item as LedgerItem>::Key) -> Option<Self::Item> {
        let path = self.item_path(key);

        if path.is_file() {
            let s = fs::read_to_string(&path);
            match s {
                Ok(s) => Some(serde_json::from_str(&s).unwrap()),
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
}
