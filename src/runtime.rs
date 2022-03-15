use ahash::{self, AHasher};
// use rustc_hash::FxHasher;

mod variant;
pub mod strings;
pub mod ops;
pub mod types;
pub mod vm;
pub mod module;
pub mod errors;

mod tests;

pub use strings::STRING_TABLE;
pub use variant::{Variant, VariantKey};
pub use vm::VirtualMachine;

// Default Hasher

pub type DefaultHasher = AHasher;
pub type DefaultBuildHasher = ahash::RandomState;
