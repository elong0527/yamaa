#![cfg(feature = "yaml")]
//! Window operations through the public file engine: the table a window
//! orders by a source field counts that field against the fixed column
//! limit. Numbering and rank (WINDOW-6), the window filter's operands, and
//! baseline_flag's eligibility and tie-break terms (WINDOW-9) are cases of
//! `tests/fixtures/diagnostics/windows.json`.
use std::fs;
use yamaa_application::file_application;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::domain_entry::Request;

struct Fixture(crate::support::Scratch);
impl Fixture {
    fn new(specification: &str, input: &str) -> Self {
        let root = crate::support::Scratch::new("window-semantics");
        fs::write(root.join("spec.yaml"), specification).unwrap();
        fs::write(root.join("input.csv"), input).unwrap();
        Self(root)
    }
    fn domain(&self) -> Result<file_application::Domain, Vec<Diagnostic>> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        crate::support::study::domain(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            crate::support::study::identity("window-semantics", specification),
        )
    }
    /// The run reports nothing and saves the artifact this returns.
    fn saved(&self) -> String {
        let mut domain = self.domain().unwrap_or_else(|issues| panic!("{issues:?}"));
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        assert!(domain.save().unwrap());
        fs::read_to_string(self.0.join("out.csv")).unwrap()
    }
}

/// Already data: `schema-baseline-tie-break` states that the latest time
/// wins, a missing time sorts lowest, and a sequence number breaks a tie on
/// the time (WINDOW-9).
#[test]
fn baseline_tie_break_terms_reproduce_their_benchmark() {
    crate::support::benchmarks::reproduce("schema-baseline-tie-break");
}

/// `width` output columns: a literal key and literal fillers, then `W`, a
/// column-level `row_number` ordered by the driver's source field `SRC.V`.
fn wide(width: usize) -> String {
    let fillers = (0..width - 1)
        .map(|index| format!("C{index}"))
        .collect::<Vec<_>>();
    let mut specification = format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [C0]\n\
         input: {{SRC: {{path: input.csv, types: {{V: int}}}}}}\n\
         output: {{path: out.csv, columns: [{}, W]}}\n\
         columns:\n",
        fillers.join(", ")
    );
    for name in &fillers {
        specification.push_str(&format!("  - {{name: {name}, type: int, derivation: 1}}\n"));
    }
    specification.push_str(
        "  - {name: W, type: int, derivation: {row_number: {window: {order_by: [SRC.V]}}}}\n\
         rows: [{id: records, dataset: SRC}]\n",
    );
    specification
}

/// Seam: 1,025 generated output columns, which a case would write out in
/// full. A window ordered by a source field orders a table of the output
/// columns and that field, 1,026 columns, past the former 1,024-column
/// limit (#4773): the run builds and saves every column.
#[test]
fn a_window_over_a_table_past_the_former_column_limit_builds() {
    let fixture = Fixture::new(&wide(1025), "V\n7\n");
    let header = (0..1024)
        .map(|index| format!("C{index}"))
        .chain(std::iter::once("W".to_owned()))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(
        fixture.saved(),
        format!("{header}\n{}\n", ["1"; 1025].join(","))
    );
}
