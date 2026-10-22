#![cfg(feature = "yaml")]
//! Window operations through the public file engine. Numbering and rank
//! order exact values within each partition and give tied rows one rank
//! (WINDOW-6); a window filter evaluates both operands of `AND`;
//! baseline_flag judges each row against its own reference and orders
//! same-date rows by its tie-break terms (WINDOW-9); and the table a window
//! orders by a source field counts that field against the fixed column
//! limit.
use std::fs;
use yamaa_adapters::file_application;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::{domain_entry::Request, json::Value};

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
        file_application::domain(
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
    /// The one issue the run reports; nothing is published.
    fn issue(&self) -> Value {
        let issues = match self.domain() {
            Ok(mut domain) => {
                assert!(domain.output().is_none());
                assert!(domain.save().is_err());
                domain.issues()
            }
            Err(issues) => issues,
        };
        assert!(!self.0.join("out.csv").exists());
        let [issue] =
            <[Diagnostic; 1]>::try_from(issues).unwrap_or_else(|issues| panic!("{issues:?}"));
        crate::support::study::issue_row(&issue)
    }
}

/// `row_number`, competition `rank` and dense `rank` over one named window
/// that partitions by `G` and orders by `order`; `V` holds `kind` values.
fn numbering(kind: &str, order: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\n\
         input: {{SRC: {{path: input.csv, types: {{ID: int, V: {kind}}}}}}}\n\
         windows:\n\
         \x20 ORDER: {{group_by: [G], order_by: [{order}]}}\n\
         columns:\n\
         \x20 - {{name: ID, type: int, derivation: SRC.ID}}\n\
         \x20 - {{name: G, type: str, derivation: SRC.G}}\n\
         \x20 - {{name: V, type: {kind}, derivation: SRC.V}}\n\
         \x20 - {{name: SEQ, type: int, derivation: {{row_number: {{window: ORDER}}}}}}\n\
         \x20 - {{name: RANK, type: int, derivation: {{rank: {{window: ORDER}}}}}}\n\
         \x20 - {{name: DENSE, type: int, derivation: {{rank: {{method: dense, window: ORDER}}}}}}\n\
         output: {{path: out.csv, columns: [ID, G, V, SEQ, RANK, DENSE]}}\n"
    )
}

/// Numbering and rank compare integers above 2^53 exactly and floats
/// numerically, so -0.0 and 0.0 tie in rank. A missing `G` forms one
/// partition, two missing values tie, and `nulls` places missing values
/// first or last whatever the direction (WINDOW-6, WINDOW-3, VALUE-22).
#[test]
fn numbering_and_rank_keep_exact_order_null_placement_and_ties_within_partitions() {
    // Partition `a` holds 2^53, 2^53 + 1 twice and a missing value; rows 2
    // and 5 form the missing partition.
    let integers = "ID,G,V\n1,a,9007199254740992\n2,,\n3,a,9007199254740993\n4,a,\n\
                    5,,\n6,a,9007199254740993\n";
    let fixture = Fixture::new(
        &numbering("int", "{variable: V, direction: desc, nulls: last}"),
        integers,
    );
    assert_eq!(
        fixture.saved(),
        "ID,G,V,SEQ,RANK,DENSE\n1,a,9007199254740992,3,3,2\n2,,,1,1,1\n\
         3,a,9007199254740993,1,1,1\n4,a,,4,4,3\n5,,,2,1,1\n6,a,9007199254740993,2,1,1\n"
    );
    // Missing first under the same descending direction.
    let fixture = Fixture::new(
        &numbering("int", "{variable: V, direction: desc, nulls: first}"),
        integers,
    );
    assert_eq!(
        fixture.saved(),
        "ID,G,V,SEQ,RANK,DENSE\n1,a,9007199254740992,4,4,3\n2,,,1,1,1\n\
         3,a,9007199254740993,2,2,2\n4,a,,1,1,1\n5,,,2,1,1\n6,a,9007199254740993,3,2,2\n"
    );
    let fixture = Fixture::new(
        &numbering("float", "V"),
        "ID,G,V\n0,x,-0.0\n1,x,-3.0\n2,x,0.0\n3,x,2.0\n",
    );
    assert_eq!(
        fixture.saved(),
        "ID,G,V,SEQ,RANK,DENSE\n0,x,-0,2,2,2\n1,x,-3,1,1,1\n2,x,0,3,2,2\n3,x,2,4,4,3\n"
    );
}

/// A window filter evaluates both operands of `AND`: a `FALSE` left operand
/// does not spare the right one, whose `int` and text operands do not
/// compare (FORMULA-5).
#[test]
fn window_filter_evaluates_both_operands_of_and() {
    let fixture = Fixture::new(
        "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\n\
         input: {SRC: {path: input.csv, types: {ID: int, V: int}}}\n\
         columns:\n\
         \x20 - {name: ID, type: int, derivation: SRC.ID}\n\
         \x20 - {name: V, type: int, derivation: SRC.V}\n\
         \x20 - name: SEQ\n\
         \x20   type: int\n\
         \x20   derivation: {row_number: {window: {order_by: [V], filter: \"FALSE AND V > 'x'\"}}}\n\
         output: {path: out.csv, columns: [ID, V, SEQ]}\n",
        "ID,V\n1,1\n",
    );
    assert_eq!(
        fixture.issue(),
        yamaa_adapters::json!({
            "phase": "validation",
            "condition": "incompatible_input_type",
            "rule": "FORMULA-5",
            "spec_paths": ["columns.SEQ.derivation.row_number.window.filter"],
            "context": {"left_type": "int", "right_type": "str"}
        })
    );
}

/// A baseline flag over `ADT` bounded by `TRTSDT`, for the never-missing
/// `ID`; `window` is its window mapping.
fn eligibility(window: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\n\
         input: {{SRC: {{path: input.csv, types: {{ID: int, ELIGIBLE: int, ADT: date, TRTSDT: date}}}}}}\n\
         columns:\n\
         \x20 - {{name: ID, type: int, derivation: SRC.ID}}\n\
         \x20 - {{name: G, type: str, derivation: SRC.G}}\n\
         \x20 - {{name: ELIGIBLE, type: int, derivation: SRC.ELIGIBLE}}\n\
         \x20 - {{name: ADT, type: date, derivation: SRC.ADT}}\n\
         \x20 - {{name: TRTSDT, type: date, derivation: SRC.TRTSDT}}\n\
         \x20 - name: BLFL\n\
         \x20   type: str\n\
         \x20   derivation:\n\
         \x20     baseline_flag: {{date: ADT, reference: TRTSDT, source: ID, window: {window}}}\n\
         output: {{path: out.csv, columns: [ID, BLFL]}}\n"
    )
}

/// A row is eligible for baseline_flag only when its date is present and
/// falls at or before its own `reference`; a missing reference leaves it
/// ineligible. A window filter removes a later row or a same-date twin from
/// the search, and the row it removes stays in the output with a missing
/// flag (WINDOW-9, WINDOW-4).
#[test]
fn baseline_flag_judges_each_row_by_its_own_reference_and_skips_filtered_rows() {
    // Row 2 falls after its own reference and row 3 has no date, so row 1
    // is the baseline of the missing partition. Row 4 of `b` has no
    // reference; row 6 is later than row 5 and still on its own reference.
    let fixture = Fixture::new(
        &eligibility("{group_by: [G]}"),
        "ID,G,ELIGIBLE,ADT,TRTSDT\n1,,1,2025-01-01,2025-01-03\n2,,1,2025-01-03,2025-01-02\n\
         3,,1,,2025-01-03\n4,b,1,2025-01-02,\n5,b,1,2025-01-02,2025-01-02\n\
         6,b,1,2025-01-03,2025-01-03\n",
    );
    assert_eq!(fixture.saved(), "ID,BLFL\n1,Y\n2,\n3,\n4,\n5,\n6,Y\n");
    let filtered = eligibility("{filter: 'ELIGIBLE > 0'}");
    for (rows, expected) in [
        // The excluded January 6 row neither takes the baseline from
        // January 2 nor ties with its January 6 twin.
        (
            "1,,1,2025-01-02,2025-01-10\n2,,0,2025-01-06,2025-01-10\n",
            "1,Y\n2,\n",
        ),
        (
            "1,,1,2025-01-06,2025-01-10\n2,,0,2025-01-06,2025-01-10\n",
            "1,Y\n2,\n",
        ),
        // A date after the reference and a missing date are never eligible.
        ("1,,1,2025-01-20,2025-01-10\n2,,1,,2025-01-10\n", "1,\n2,\n"),
        (
            "1,,1,2025-01-06,2025-01-10\n2,,1,2025-01-02,2025-01-10\n",
            "1,Y\n2,\n",
        ),
        // Row 2 shares row 1's date but falls after its own reference.
        (
            "1,,1,2025-01-06,2025-01-10\n2,,1,2025-01-06,2025-01-02\n",
            "1,Y\n2,\n",
        ),
        // Ineligible and filtered rows do not end the search.
        (
            "1,,1,2025-01-20,2025-01-10\n2,,1,,2025-01-10\n3,,1,2025-01-06,2025-01-10\n",
            "1,\n2,\n3,Y\n",
        ),
        (
            "1,,0,2025-01-20,2025-01-10\n2,,1,,2025-01-10\n3,,1,2025-01-06,2025-01-10\n",
            "1,\n2,\n3,Y\n",
        ),
        // A date equal to its own reference is eligible.
        ("1,,1,2025-01-06,2025-01-06\n", "1,Y\n"),
    ] {
        let fixture = Fixture::new(&filtered, &format!("ID,G,ELIGIBLE,ADT,TRTSDT\n{rows}"));
        assert_eq!(fixture.saved(), format!("ID,BLFL\n{expected}"), "{rows}");
    }
}

/// A baseline flag within `G` whose same-date rows are ordered by `terms`,
/// the window order's terms after its date.
fn tie_break(terms: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\n\
         input: {{SRC: {{path: input.csv, types: {{ID: int, ADT: date, TRTSDT: date, TM: int, SEQ: int}}}}}}\n\
         columns:\n\
         \x20 - {{name: ID, type: int, derivation: SRC.ID}}\n\
         \x20 - {{name: G, type: str, derivation: SRC.G}}\n\
         \x20 - {{name: ADT, type: date, derivation: SRC.ADT}}\n\
         \x20 - {{name: TRTSDT, type: date, derivation: SRC.TRTSDT}}\n\
         \x20 - {{name: TM, type: int, derivation: SRC.TM}}\n\
         \x20 - {{name: SEQ, type: int, derivation: SRC.SEQ}}\n\
         \x20 - name: BLFL\n\
         \x20   type: str\n\
         \x20   derivation:\n\
         \x20     baseline_flag: {{date: ADT, reference: TRTSDT, source: ID, \
         window: {{group_by: [G], order_by: [ADT, {terms}]}}}}\n\
         output: {{path: out.csv, columns: [ID, BLFL]}}\n"
    )
}

/// Eligible rows sharing the latest date are ordered by the tie-break terms,
/// each with its own direction and missing placement, and the last is the
/// baseline; rows still equal on every term fail, reporting the date, the
/// number of tied rows and the partition (WINDOW-9, VALUE-22).
#[test]
fn baseline_tie_break_terms_pick_the_last_same_date_row_or_fail_when_still_tied() {
    // Rows 1 to 3 share January 3 with descending SEQ; row 4 is earlier.
    let rows = |times: [&str; 3]| {
        format!(
            "ID,G,ADT,TRTSDT,TM,SEQ\n1,a,2025-01-03,2025-01-05,{},3\n\
             2,a,2025-01-03,2025-01-05,{},2\n3,a,2025-01-03,2025-01-05,{},1\n\
             4,a,2025-01-01,2025-01-05,2300,9\n",
            times[0], times[1], times[2]
        )
    };
    let flagged = |id: usize| {
        (1..=4).fold(String::from("ID,BLFL\n"), |csv, row| {
            csv + &format!("{row},{}\n", if row == id { "Y" } else { "" })
        })
    };
    for (terms, id) in [
        // The latest time wins; a missing time sorts lowest.
        ("TM", 1),
        // Descending prefers the smallest time, and missing stays lowest,
        // which is now last.
        ("{variable: TM, direction: desc}", 3),
        // A declared placement puts the missing time last.
        ("{variable: TM, nulls: last}", 3),
        // A sequence number instead of a time.
        ("SEQ", 1),
        ("{variable: SEQ, direction: desc}", 3),
    ] {
        let fixture = Fixture::new(&tie_break(terms), &rows(["900", "800", ""]));
        assert_eq!(fixture.saved(), flagged(id), "{terms}");
    }
    // Rows 1 and 2 are equal on the time; the sequence breaks the tie in
    // favour of row 1.
    let tied = rows(["900", "900", "800"]);
    let mut issue = Fixture::new(&tie_break("TM"), &tied).issue();
    // The facts WINDOW-9 names: the shared date, the number of tied rows and
    // the partition.
    let context = issue["context"].clone();
    issue["context"] = yamaa_adapters::json!({"date": context["date"].clone(),
        "match_count": context["match_count"].clone(), "keys": context["keys"].clone()});
    assert_eq!(
        issue,
        yamaa_adapters::json!({
            "phase": "derivation",
            "condition": "ambiguous_baseline",
            "rule": "WINDOW-9",
            "spec_paths": ["columns.BLFL.derivation.baseline_flag"],
            "context": {"date": "2025-01-03", "match_count": 2, "keys": [{"G": "a"}]}
        })
    );
    assert_eq!(
        Fixture::new(&tie_break("TM, SEQ"), &tied).saved(),
        flagged(1)
    );
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

/// A window ordered by a source field orders a table of the output columns
/// and that field, and the fixed limit of 1,024 columns holds for that
/// table: 1,023 output columns build, while 1,024 are refused at the
/// window's own path.
#[test]
fn window_context_table_counts_its_source_field_against_the_column_limit() {
    let fixture = Fixture::new(&wide(1023), "V\n7\n");
    let header = (0..1022)
        .map(|index| format!("C{index}"))
        .chain(std::iter::once("W".to_owned()))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(
        fixture.saved(),
        format!("{header}\n{}\n", ["1"; 1023].join(","))
    );
    assert_eq!(
        Fixture::new(&wide(1024), "V\n7\n").issue(),
        yamaa_adapters::json!({
            "phase": "engine",
            "condition": "engine_rejected",
            "rule": null,
            "spec_paths": ["columns.W.derivation.row_number"],
            "context": {
                "step": "execute",
                "code": "execution_limit",
                "resource": "columns",
                "limit": 1024,
                "actual": 1025
            }
        })
    );
}
