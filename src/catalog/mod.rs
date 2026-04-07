pub mod cache;
pub mod capability_resolver;
pub mod client;
pub mod registry;
pub mod resolve;

pub const DEFAULT_CACHE_POLICY: &str = "workspace-local";
pub const CACHE_ROOT_DIR: &str = "state/cache/catalogs";
