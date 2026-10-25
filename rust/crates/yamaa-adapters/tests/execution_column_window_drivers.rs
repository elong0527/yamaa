#![cfg(feature = "yaml")]
//! A column-level window over the rows of several row templates (WINDOW-11).
//! Its refusals, which `check` decides from the declarations as `domain`
//! does (WINDOW-11, NAME-3, NAME-4), and its builds over templates that
//! share the driver are cases of `tests/fixtures/diagnostics/window_drivers.json`.

/// Already data: `adam-advs-windows` builds one column window over the rows
/// of two drivers, though each of its partitions holds one driver's rows and
/// its output reads the rank only through `ANL01FL`.
#[test]
fn a_column_window_over_two_drivers_rows_reproduces_its_benchmark() {
    crate::support::benchmarks::reproduce("adam-advs-windows");
}
