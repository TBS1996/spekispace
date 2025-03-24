#[cfg(feature = "fs")]
mod fs;

#[cfg(feature = "fs")]
mod snapfs;

#[cfg(feature = "fs")]
pub use fs::{FsProvider, FsTime};

#[cfg(feature = "browserfs")]
mod browserfs;

#[cfg(feature = "browserfs")]
pub use browserfs::BrowserFsProvider;

#[cfg(feature = "dexie")]
mod dexie;

#[cfg(feature = "dexie")]
pub use dexie::{DexieProvider, WasmTime};
