#[cfg(feature = "fs")]
mod fs;

#[cfg(feature = "fs")]
pub use fs::paths;
#[cfg(feature = "fs")]
pub use fs::FileProvider;

#[cfg(feature = "browserfs")]
mod browserfs;

#[cfg(feature = "browserfs")]
pub use browserfs::BrowserFsProvider;

#[cfg(feature = "dexie")]
mod dexie;

#[cfg(feature = "dexie")]
pub use dexie::{DexieProvider, WasmTime};
