#[cfg(feature = "fs")]
mod fs;

#[cfg(feature = "fs")]
pub use fs::FileProvider;

#[cfg(feature = "fs")]
pub use fs::paths;

#[cfg(feature = "browserfs")]
mod browserfs;

#[cfg(feature = "browserfs")]
pub use browserfs::IndexBaseProvider;

#[cfg(feature = "dexie")]
mod dexie;

#[cfg(feature = "dexie")]
pub use dexie::DexieProvider;
