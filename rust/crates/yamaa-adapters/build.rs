#[cfg(not(feature = "readstat"))]
fn main() {}

#[cfg(feature = "readstat")]
fn main() {
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
