//! The safe client of `yamaa-boundary` over the engine (#3702; the design on
//! #2870, section 3.3): each request of the shared truth's `interface`
//! family, sent through a binding's gate to the engine's entry, negotiates
//! to its authored outcome; the first acceptance opens the gate and nothing
//! closes it; a handle the client owns is given back when it drops; and
//! the table of no issues a binding attaches to its own refusals has the
//! issues columns.
use arrow_array::RecordBatchReader;
use yamaa_boundary::client::Gate;
use yamaa_boundary::{Closed, HandleKind, Outcome, Value, BOUNDARY_VERSION};

use crate::support::{family, host_request, outcome_record, reader, resolved, serial};

/// Negotiate `request` through `gate` with the engine's entry.
fn negotiated(gate: &Gate, request: &Value) -> Outcome {
    // SAFETY: the entry is the engine's negotiation entry.
    unsafe { gate.negotiate(request, yamaa_engine::negotiate) }
}

#[test]
fn every_interface_case_negotiates_to_its_authored_outcome_through_the_client() {
    let interface = family("interface");
    let generated = [(
        "the generated request",
        &interface["request"],
        &interface["accepted"],
    )];
    let crafted = interface["cases"].as_array().unwrap().iter().map(|case| {
        let case_name = case["case"].as_str().unwrap();
        (case_name, &case["request"], &case["outcome"])
    });
    for (case, value, outcome) in generated.into_iter().chain(crafted) {
        let answered = negotiated(&Gate::new(), &host_request(&resolved(value)));
        assert_eq!(outcome_record(&answered), resolved(outcome), "{case}");
    }
}

/// The gate refuses until a negotiation is accepted, and a refused
/// negotiation, before or after, changes nothing.
#[test]
fn the_first_acceptance_opens_the_gate_and_nothing_closes_it() {
    let gate = Gate::new();
    let generated = host_request(&resolved(&family("interface")["request"]));
    let mismatch = Value::Map(vec![(
        "version".into(),
        Value::Integer(i64::from(BOUNDARY_VERSION) + 1),
    )]);
    let open = |gate: &Gate| gate.pass().map(|_| ());
    let mut steps = vec![("before any negotiation", open(&gate))];
    for (step, value) in [
        ("after a refusal", &mismatch),
        ("after an acceptance", &generated),
        ("after a later refusal", &mismatch),
        ("after a later acceptance", &generated),
    ] {
        negotiated(&gate, value);
        steps.push((step, open(&gate)));
    }
    assert_eq!(
        steps,
        [
            ("before any negotiation", Err(Closed)),
            ("after a refusal", Err(Closed)),
            ("after an acceptance", Ok(())),
            ("after a later refusal", Ok(())),
            ("after a later acceptance", Ok(())),
        ]
    );
}

#[test]
fn a_handle_the_client_owns_is_given_back_when_it_drops() {
    let _serial = serial();
    let gate = Gate::new();
    negotiated(
        &gate,
        &host_request(&resolved(&family("interface")["request"])),
    );
    let engine = gate.pass().unwrap();
    let plans = |census: yamaa_boundary::client::Census| {
        census
            .handles
            .into_iter()
            .find(|(kind, _)| *kind == HandleKind::Plans)
            .map(|(_, count)| count)
    };
    let before = plans(engine.census());
    let owned = engine.plans().unwrap();
    let held = plans(engine.census());
    drop(owned);
    assert_eq!(
        (held, plans(engine.census())),
        (before.map(|count| count + 1), before),
        "a plans holder the client owns, held and dropped"
    );
}

#[test]
fn the_table_of_no_issues_has_the_issues_columns_and_no_rows() {
    // The table a binding attaches to a refusal of its own: the published
    // issues columns (`yamaa::ISSUE_COLUMNS`, the `issues` table of
    // rust/interface/bindings.yaml), without a row.
    let _serial = serial();
    let gate = Gate::new();
    negotiated(
        &gate,
        &host_request(&resolved(&family("interface")["request"])),
    );
    let engine = gate.pass().unwrap();
    let issues = engine.no_issues().unwrap();
    let reader = reader(engine.export(issues.handle()).unwrap().into_raw());
    let columns: Vec<String> = reader
        .schema()
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .collect();
    let rows: usize = reader.map(|batch| batch.unwrap().num_rows()).sum();
    assert_eq!(
        (columns, rows),
        (yamaa::ISSUE_COLUMNS.map(str::to_owned).to_vec(), 0),
        "the table of no issues"
    );
}
