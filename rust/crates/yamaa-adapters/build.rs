#[cfg(not(feature = "readstat"))]
fn main() {}

#[cfg(feature = "readstat")]
fn main() {
    let root = "vendor/readstat";
    let mut build = cc::Build::new();
    build.include(root).std("c11");
    // Ordinary builds stay quiet: a host's own compiler may warn where CI's
    // do not, and a warning must not fail an installation. Pull request CI
    // sets `YAMAA_READSTAT_WARNINGS=deny` (`tools.ci.readstat`), which turns
    // `-Wall -Wextra -Wformat-security` into errors, so a new warning in this
    // C fails there instead (#4871).
    println!("cargo:rerun-if-env-changed=YAMAA_READSTAT_WARNINGS");
    match std::env::var("YAMAA_READSTAT_WARNINGS").as_deref() {
        Ok("deny") => build
            .warnings(true)
            .warnings_into_errors(true)
            .flag_if_supported("-Wformat-security"),
        Ok(other) => panic!("YAMAA_READSTAT_WARNINGS is `deny` or unset, not `{other}`"),
        Err(_) => build.warnings(false),
    };
    harden(&mut build);
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
    watch(std::path::Path::new(root));
}

/// The hardening flags of this C, which parses untrusted files. MSVC already
/// compiles with `/GS`. GCC and Clang on Unix targets get a stack protector
/// and, in an optimized build, fortified libc calls: glibc warns that
/// `_FORTIFY_SOURCE` needs optimization, and some compilers predefine it, so
/// it is undefined first. MinGW is left out: its
/// stack protector and fortified calls need `libssp`, which the Rust link line
/// does not name. Each flag is probed, so a compiler without it builds as
/// before; `CFLAGS` comes after these and can still override them.
#[cfg(feature = "readstat")]
fn harden(build: &mut cc::Build) {
    let unix = std::env::var("CARGO_CFG_TARGET_FAMILY")
        .is_ok_and(|family| family.split(',').any(|family| family == "unix"));
    if !unix || build.get_compiler().is_like_msvc() {
        return;
    }
    build.flag_if_supported("-fstack-protector-strong");
    if std::env::var("OPT_LEVEL").is_ok_and(|level| level != "0") {
        build
            .flag_if_supported("-U_FORTIFY_SOURCE")
            .define("_FORTIFY_SOURCE", "2");
    }
}

/// Rerun when a ReadStat source file changes. Cargo compares a watched
/// directory's own modification time too, and unpacking a source package
/// gives directories the time of unpacking, so each file is watched instead:
/// an R installation that shares the Cargo build of an earlier one from the
/// same package then rebuilds nothing.
#[cfg(feature = "readstat")]
fn watch(path: &std::path::Path) {
    if path.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(path)
            .and_then(|entries| entries.map(|entry| Ok(entry?.path())).collect())
            .expect("the ReadStat sources are readable");
        entries.sort();
        for entry in entries {
            watch(&entry);
        }
    } else {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
