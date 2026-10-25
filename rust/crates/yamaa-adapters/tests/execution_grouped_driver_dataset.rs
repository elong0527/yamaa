#![cfg(feature = "yaml")]
//! A driver field read outside the groups of a grouped row template, and a
//! column read of an ungrouped driver beside another driver's groups. The
//! refusals are cases of `fixtures/diagnostics/check_row_scopes.json`.

/// Already data: `schema-rows-group-scope` states the CSV it saves. The home
/// template averages its readings and states its own visit, so only the
/// clinic template completes the visit from the column-level derivation. No
/// template groups the clinic measurements, so each clinic row reads its own
/// measurement.
#[test]
fn column_read_of_an_ungrouped_driver_ignores_another_drivers_groups() {
    crate::support::benchmarks::reproduce("schema-rows-group-scope");
}
