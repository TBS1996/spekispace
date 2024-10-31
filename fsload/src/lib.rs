use dirs::home_dir;
use rayon::prelude::*;
use sanitize_filename::sanitize;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::marker::PhantomData;
use std::{
    collections::BTreeSet,
    fs::{self, read_to_string},
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
struct CacheInfo<T: FsLoad> {
    path: PathBuf,
    dependents: BTreeSet<Uuid>,
    #[serde(skip)]
    _marker: PhantomData<T>,
}

impl<T: FsLoad> CacheInfo<T> {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            dependents: BTreeSet::default(),
            _marker: PhantomData,
        }
    }

    /// Ensures all the depencies of id, has id as a dependent.
    fn cache_dependents(ty: &T) {
        for dpy in ty.dependencies() {
            if let Some(mut info) = Self::load(dpy) {
                if info.dependents.insert(ty.id()) {
                    info.persist(dpy);
                } else {
                }
            }
        }
    }

    fn cache_path(id: Uuid, path: PathBuf) {
        //        std::thread::sleep(std::time::Duration::from_millis(250));
        match Self::load(id) {
            Some(mut info) => {
                if &info.path != &path {
                    info.path = path;
                    info.persist(id);
                }
            }
            None => {
                Self::new(path).persist(id);
            }
        }
    }

    fn load(id: Uuid) -> Option<Self> {
        let cache_path = Self::cache_dir().join(id.to_string());
        let s: String = read_to_string(&cache_path).ok()?;
        let info: Self = toml::from_str(&s).ok()?;
        Some(info)
    }

    fn cache_dir() -> PathBuf {
        let p = home_dir().unwrap().join(".cache").join(T::type_name());
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn persist(self, id: Uuid) {
        let p = Self::cache_dir().join(id.to_string());
        let s: String = toml::to_string(&self).unwrap();
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(&mut s.as_bytes()).unwrap();
    }
}

pub trait FsLoad: Serialize + DeserializeOwned + Send {
    fn id(&self) -> Uuid;

    fn dependencies(&self) -> BTreeSet<Uuid> {
        Default::default()
    }

    fn dependents(&self) -> BTreeSet<Uuid> {
        Self::verify_dependents::<Self>(self.id());
        let info: Option<CacheInfo<Self>> = CacheInfo::load(self.id());
        info.map(|info| info.dependents).unwrap_or_default()
    }

    fn save(&self) {
        let path = Self::load_path_with_cache::<Self>(Self::save_paths(), self.id())
            .unwrap_or_else(|| Self::save_paths()[0].join(self.file_name()));
        let s: String = toml::to_string(&self).unwrap();
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&mut s.as_bytes()).unwrap();

        CacheInfo::<Self>::cache_path(self.id(), path);
        CacheInfo::cache_dependents(self);
    }

    fn save_at(&self, dir: &Path) {
        assert!(
            dir.is_dir(),
            "pass in the directory where the file should reside"
        );
        assert!(
            CacheInfo::<Self>::load(self.id()).is_none(),
            "card already exists"
        );

        let base_filename = sanitize(&self.file_name()).replace(" ", "_");
        let mut path = dir.join(&base_filename);
        path.set_extension("toml");

        let mut counter = 1;
        while path.exists() {
            let new_name = format!("{}({})", &base_filename, counter);
            path = dir.join(&new_name);
            path.set_extension("toml");
            counter += 1;
        }

        let s: String = toml::to_string(&self).unwrap();
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&mut s.as_bytes()).unwrap();
        CacheInfo::<Self>::cache_path(self.id(), path.to_path_buf());
        CacheInfo::cache_dependents(self);
    }

    /// Name of file when first saved, will not be updated later
    /// Can be changed manually later, cache will find it and save its new location
    /// as long as it's saved in a subdir of any of dirs in [`Self::dir_path`].
    fn file_name(&self) -> String {
        self.id().to_string()
    }

    /// Name of the type of object. Defaults to structs name.
    /// By default the instances will be saved in "~/.local/share/{type_name}/"
    /// The cache will be stored in "~/.cache/{type_name}/"
    fn type_name() -> String {
        std::any::type_name::<Self>()
            .to_string()
            .split("::")
            .last()
            .unwrap()
            .to_string()
    }

    /// The path where this instance is stored, if its stored.
    fn path(&self) -> Option<PathBuf> {
        Self::load_path_with_cache::<Self>(Self::save_paths(), self.id())
    }

    /// Loads a specific instance by its ID.
    fn load(id: Uuid) -> Option<Self> {
        let path = Self::load_path_with_cache::<Self>(Self::save_paths(), id)?;
        let s = read_to_string(&path).unwrap();
        let s: Self = toml::from_str(&s).unwrap();
        CacheInfo::cache_dependents(&s);
        Some(s)
    }

    /// Loads all the objects that are stored under any of the dirs in dir_path
    fn load_all() -> Vec<Self> {
        let mut vec = vec![];

        for p in Self::save_paths() {
            for obj in load_all(&p) {
                vec.push(obj);
            }
        }

        vec
    }

    /// The root dirs of where instances are stored. This is where the cache will look
    fn save_paths() -> Vec<PathBuf> {
        let path = dirs::home_dir()
            .unwrap()
            .join(".local")
            .join("share")
            .join(Self::type_name());
        std::fs::create_dir_all(&path).unwrap();
        vec![path]
    }

    /// Returns path of file if it exists in the cache and a file is on that path.
    fn try_load(id: Uuid) -> Option<PathBuf> {
        CacheInfo::<Self>::load(id)
            .map(|info| info.path)
            .filter(|path| path.exists())
    }

    fn verify_dependents<T: FsLoad>(id: Uuid) {
        if let Some(mut info) = CacheInfo::<Self>::load(id) {
            let len = info.dependents.len();
            info.dependents
                .retain(|dependent| T::load(*dependent).unwrap().dependencies().contains(&id));

            if info.dependents.len() != len {
                info.persist(id);
            }
        }
    }

    fn load_path_with_cache<T: FsLoad>(dir_paths: Vec<PathBuf>, id: Uuid) -> Option<PathBuf> {
        match Self::try_load(id) {
            some @ Some(_) => some,
            None => {
                Self::sync_cache::<T>(dir_paths);
                Self::try_load(id)
            }
        }
    }

    fn sync_cache<T: FsLoad>(dir_paths: Vec<PathBuf>) {
        let mut vec = vec![];

        for dir_path in dir_paths {
            for path in get_all_file_paths(&dir_path) {
                let s = std::fs::read_to_string(&path).unwrap();
                let obj: T = toml::from_str(&s).unwrap();
                CacheInfo::<Self>::cache_path(obj.id(), path);
                vec.push(obj);
            }
        }

        // we do this separately in case the get_depencies() implementaton also calls the cache
        // that would cause an infinite loop
        for o in vec {
            CacheInfo::cache_dependents(&o);
        }
    }
}

fn load_all<T: DeserializeOwned + FsLoad + Send>(dir: &Path) -> Vec<T> {
    let paths = get_all_file_paths(dir);

    let vec: Vec<T> = paths
        .into_par_iter()
        .map(|path| {
            let s = std::fs::read_to_string(&path).unwrap();
            let obj: T = toml::from_str(&s).unwrap();
            CacheInfo::<T>::cache_path(obj.id(), path);
            obj
        })
        .collect();

    for o in &vec {
        CacheInfo::cache_dependents(o);
    }

    vec
}

fn get_all_file_paths(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                Some(path.to_path_buf())
            } else {
                None
            }
        })
        .par_bridge()
        .collect()
}
