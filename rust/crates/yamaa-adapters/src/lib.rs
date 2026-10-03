//! Infrastructure adapters. The bootstrap resource is embedded at build time.
#![forbid(unsafe_code)]

/// Exercise resource inclusion without depending on the checkout at runtime.
pub fn installation_resource() -> &'static str {
    include_str!("../resources/installation.txt")
}
