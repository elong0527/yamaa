fn main() {
    detect_parquet_major();
    #[cfg(feature = "readstat")]
    build_readstat();
}

/// Emit the `yamaa_parquet_57` cfg when the resolved parquet graph is the
/// 57 release, whose APIs differ from the current release (see
/// `src/storage_api.rs`). An embedder that resolves the graph outside a
/// Cargo lock can set `YAMAA_PARQUET_MAJOR` instead; without either signal
/// the current API is assumed and nothing is emitted.
fn detect_parquet_major() {
    println!("cargo::rustc-check-cfg=cfg(yamaa_parquet_57)");
    println!("cargo:rerun-if-env-changed=YAMAA_PARQUET_MAJOR");
    let major = match std::env::var("YAMAA_PARQUET_MAJOR") {
        Ok(value) => value.trim().parse::<u64>().ok(),
        Err(_) => lockfile_parquet_major(),
    };
    if major == Some(57) {
        println!("cargo:rustc-cfg=yamaa_parquet_57");
    }
}

/// Read the parquet major version from the workspace lock, if one exists.
fn lockfile_parquet_major() -> Option<u64> {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")?;
    let lock = std::path::Path::new(&manifest_dir).join("../../Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    let contents = std::fs::read_to_string(lock).ok()?;
    for package in contents.split("[[package]]") {
        let mut name = None;
        let mut version = None;
        for line in package.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            match key.trim() {
                "name" => name = Some(value),
                "version" => version = Some(value),
                _ => {}
            }
        }
        if name == Some("parquet") {
            return version?.split('.').next()?.parse().ok();
        }
    }
    None
}

#[cfg(feature = "readstat")]
fn build_readstat() {
    let root = "vendor/readstat";
    let mut build = cc::Build::new();
    build.include(root).std("c11").warnings(false);
    for file in [
        "readstat_bits.c",
        "readstat_convert.c",
        "readstat_error.c",
        "readstat_malloc.c",
        "readstat_metadata.c",
        "readstat_parser.c",
        "readstat_value.c",
        "readstat_variable.c",
        "sas/ieee.c",
        "sas/readstat_sas.c",
        "sas/readstat_sas7bdat_read.c",
        "sas/readstat_sas_rle.c",
        "sas/readstat_xport.c",
        "sas/readstat_xport_read.c",
        "yamaa_bridge.c",
    ] {
        build.file(format!("{root}/{file}"));
    }
    build.compile("yamaa_readstat");
    println!("cargo:rerun-if-changed={root}");
}
