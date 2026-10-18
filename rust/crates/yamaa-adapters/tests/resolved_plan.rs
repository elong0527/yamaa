#![cfg(feature = "yaml")]
//! The resolved plan graph (#2840) of authored benchmark specifications, on
//! the rows and the no-rows binding routes, and the bind rejection that a
//! failed resolution check produces.
#[allow(dead_code)]
mod support;
use std::path::{Path, PathBuf};
use support::specification_run::{prepare, schema};
use yamaa_core::{
    dataset::{
        resolved::{ColumnPhase, Grain, RegionKind, Stage, Step},
        DatasetPlan,
    },
    specification::{PreparedSpecification, SourceInterpretation},
    table::{Column, TableSchema},
    value::ColumnType,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Bind one benchmark against the schemas a run decodes before binding: the
/// fixed ODM fields, or the CSV header with the declared input types.
fn bind(case: &str) -> DatasetPlan {
    let root = root();
    let case = root.join("benchmarks").join(case);
    let captured = schema(&root.join("yaml"));
    let document = prepare(&captured, &std::fs::read(case.join("spec.yaml")).unwrap());
    let prepared = PreparedSpecification::prepare(document.model()).unwrap();
    let schemas: Vec<_> = prepared
        .sources()
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let text = std::fs::read_to_string(case.join(&source.path)).unwrap();
            let names: Vec<&str> = match prepared.source_interpretation(index) {
                Some(SourceInterpretation::Odm) => yamaa_core::odm_source::FIELDS.to_vec(),
                _ => text.lines().next().unwrap().split(',').collect(),
            };
            let columns = names
                .into_iter()
                .map(|name| Column {
                    name: name.into(),
                    kind: source
                        .types
                        .iter()
                        .find(|(field, _)| field == name)
                        .map_or(ColumnType::Str, |(_, kind)| *kind),
                })
                .collect();
            TableSchema::new(columns).unwrap()
        })
        .collect();
    prepared
        .bind_sources(&schemas.iter().collect::<Vec<_>>())
        .unwrap()
}

#[test]
fn a_derived_intermediate_resolves_its_donor_plan_as_its_own_region() {
    // QT derives RRPAIR from RR for every QT record; the third row template
    // is driven by QT.
    let plan = bind("adam-adeg-replicate-qtc-average");
    let resolved = plan.resolve().unwrap();
    assert_eq!(resolved.regions().len(), 2);
    let qt = resolved
        .relations()
        .iter()
        .position(|relation| &*relation.path == "intermediates[1]")
        .unwrap();
    let donor = resolved.relations()[qt].derivations.unwrap();
    assert_eq!(
        resolved.region(donor).kind,
        RegionKind::Donor {
            stored_columns: 13,
            typed_columns: Vec::new()
        }
    );
    let driven = resolved
        .templates()
        .iter()
        .find(|template| template.driver.named.is_some())
        .unwrap();
    assert_eq!(driven.driver.named.unwrap().index(), qt);
    // The donor copies its thirteen stored fields, then derives RRPAIR.
    let steps: Vec<_> = resolved
        .region_schedule(donor)
        .iter()
        .map(|info| info.step)
        .collect();
    assert!(matches!(steps[0], Step::Driver(_)));
    assert_eq!(
        steps[1..14]
            .iter()
            .filter(|step| matches!(step, Step::Row(_)))
            .count(),
        13
    );
    let Some(&Step::Column(rrpair)) = steps.last() else {
        panic!("the donor ends with its derivation: {steps:?}")
    };
    assert_eq!(&*resolved.column(rrpair).name, "RRPAIR");
    assert_eq!(steps.len(), 15);
    // AVAL is derived in every template's row phase, DTYPE in the column phase.
    let named = |name: &str| {
        resolved
            .region_columns(yamaa_core::dataset::resolved::RegionId::DATASET)
            .find(|&column| &*resolved.column(column).name == name)
            .unwrap()
    };
    assert_eq!(resolved.column(named("AVAL")).phase, ColumnPhase::RowOnly);
    assert_eq!(
        resolved.column(named("DTYPE")).phase,
        ColumnPhase::ColumnOnly
    );
    assert!(resolved
        .column_bindings(named("DTYPE"))
        .all(|binding| resolved.binding(binding).stage == Stage::Column));
}

#[test]
fn a_specification_without_rows_resolves_one_key_grain_template() {
    let plan = bind("adam-adsl-age-group");
    let resolved = plan.resolve().unwrap();
    assert_eq!(resolved.regions().len(), 1);
    assert_eq!(resolved.templates().len(), 1);
    assert_eq!(resolved.templates()[0].driver.grain, Grain::Keys);
    let steps: Vec<_> = resolved.schedule().iter().map(|info| info.step).collect();
    let grain = steps
        .iter()
        .position(|step| *step == Step::KeyGrain)
        .unwrap();
    // Only key bindings precede the key grain; every other column follows it.
    assert!(steps[..grain]
        .iter()
        .all(|step| !matches!(step, Step::Column(_))));
    assert_eq!(
        steps[grain..]
            .iter()
            .filter(|step| matches!(step, Step::Column(_)))
            .count(),
        resolved.columns().len() - resolved.keys().len()
    );
}

#[test]
fn a_failed_resolution_check_is_an_internal_bind_rejection() {
    use yamaa_adapters::{specification_diagnostics::failure, specification_run::Error};
    use yamaa_core::{dataset::ResolutionError, specification::BindError};
    let reply = failure(
        &Error::Bind(BindError::Resolution(ResolutionError::Capacity)),
        None,
    );
    assert!(reply.contains(r#""status":"rejected""#), "{reply}");
    assert!(reply.contains(r#""stage":"bind""#), "{reply}");
    assert!(reply.contains(r#""code":"internal""#), "{reply}");
}
