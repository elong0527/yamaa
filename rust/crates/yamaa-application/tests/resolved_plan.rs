#![cfg(feature = "yaml")]
//! The resolved plan graph (#2840) of authored benchmark specifications, on
//! the rows and the no-rows binding routes, and the bind rejection that a
//! failed resolution check produces.
use crate::support;
use std::path::{Path, PathBuf};
use support::specification_run::{compile, prepare, try_prepare};
use yamaa_core::{
    dataset::{
        resolved::{ColumnPhase, Grain, RegionId, RegionKind, Stage, Step},
        ResolvedPlan,
    },
    specification::SourceInterpretation,
    table::{Column, TableSchema},
    value::ColumnType,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Bind one benchmark against the schemas a run decodes before binding: the
/// fixed ODM fields, or the CSV header with the declared input types.
/// Binding returns the plan already resolved.
fn bind(case: &str) -> ResolvedPlan {
    let root = root();
    let case = root.join("benchmarks").join(case);
    let document = prepare(&std::fs::read(case.join("spec.yaml")).unwrap());
    let prepared = compile(&document).unwrap();
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
    let resolved = bind("adam-adeg-replicate-qtc-average");
    assert_eq!(resolved.regions().len(), 2);
    let qt = resolved
        .relations()
        .iter()
        .position(|relation| {
            // By identifier: a relation path is written in the language of
            // the benchmark, which moves to 1.0 in its own pull request.
            relation
                .named
                .as_ref()
                .is_some_and(|named| named.identifier == "QT")
        })
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
fn named_relation_indices_cannot_select_an_aggregate_or_another_region() {
    let resolved = bind("adam-adeg-replicate-qtc-average");
    let dataset = RegionId::DATASET;
    let rr = resolved.named(dataset, 0).unwrap();
    let qt = resolved.named(dataset, 1).unwrap();
    let identifier = |id| &resolved.relation(id).named.as_ref().unwrap().identifier;
    assert_eq!(identifier(rr), "RR");
    assert_eq!(identifier(qt), "QT");
    // The dataset's next relation is an aggregate, not a third intermediate.
    assert_eq!(resolved.named(dataset, 2), None);
    let donor = resolved.relation(qt).derivations.unwrap();
    let donor_rr = resolved.named(donor, 0).unwrap();
    assert_eq!(identifier(donor_rr), "RR");
    assert_ne!(rr, donor_rr);
    // A dataset-local index must not reach the donor's valid named relation.
    assert_eq!(resolved.named(dataset, donor_rr.index()), None);
    assert_eq!(resolved.named(donor, 1), None);
    for region in [dataset, donor] {
        assert_eq!(resolved.named(region, usize::MAX), None);
        assert_eq!(resolved.named(region, u32::MAX as usize), None);
        if let Ok(wrapped) = usize::try_from(u64::from(u32::MAX) + 1) {
            assert_eq!(resolved.named(region, wrapped), None);
        }
    }
}

/// Every node of every benchmark that binds from its CSV or ODM inputs
/// carries the operation of its kind.
#[test]
fn every_benchmark_node_carries_the_operation_of_its_kind() {
    let mut resolved_cases = 0;
    for case in crate::support::benchmarks::all() {
        let name = case.file_name().unwrap().to_str().unwrap().to_owned();
        let Some(resolved) = try_bind(&name) else {
            continue;
        };
        for node in resolved.exprs() {
            assert!(
                node.op.fits(node.kind),
                "{name}: {:?} holds {:?}",
                node.kind,
                node.op
            );
        }
        resolved_cases += 1;
    }
    assert!(resolved_cases > 200, "{resolved_cases} benchmarks bound");
}

/// Bind one benchmark when its specification prepares and binds against the
/// headers of CSV inputs or the ODM fields; none otherwise.
fn try_bind(case: &str) -> Option<ResolvedPlan> {
    let root = root();
    let case = root.join("benchmarks").join(case);
    let bytes = std::fs::read(case.join("spec.yaml")).ok()?;
    let document = try_prepare(&bytes).ok()?;
    let prepared = compile(&document).ok()?;
    let mut schemas = Vec::new();
    for (index, source) in prepared.sources().iter().enumerate() {
        let names: Vec<String> = match prepared.source_interpretation(index) {
            Some(SourceInterpretation::Odm) => yamaa_core::odm_source::FIELDS
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
            _ => {
                if !source.path.ends_with(".csv") {
                    return None;
                }
                let text = std::fs::read_to_string(case.join(&source.path)).ok()?;
                text.lines()
                    .next()?
                    .split(',')
                    .map(|name| name.to_owned())
                    .collect()
            }
        };
        let columns = names
            .into_iter()
            .map(|name| Column {
                kind: source
                    .types
                    .iter()
                    .find(|(field, _)| *field == name)
                    .map_or(ColumnType::Str, |(_, kind)| *kind),
                name,
            })
            .collect();
        schemas.push(TableSchema::new(columns).ok()?);
    }
    prepared
        .bind_sources(&schemas.iter().collect::<Vec<_>>())
        .ok()
}

#[test]
fn a_specification_without_rows_resolves_one_key_grain_template() {
    let resolved = bind("adam-adsl-age-group");
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
fn a_dtc_part_after_the_year_is_read_only_when_reached() {
    use yamaa_core::dataset::resolved::{Demand, ExprId, ExprKind};
    // `dtc` stops at its first missing part (TIME-19), so only the year is
    // always read; `study_day` reads both of its dates.
    let resolved = bind("sdtm-ae-partial-dates");
    let demands = |operation: &str| {
        (0..resolved.exprs().len())
            .map(|index| ExprId::try_from(index).unwrap())
            .filter(|&id| {
                let node = resolved.expr(id);
                node.kind == ExprKind::Temporal && node.path.ends_with(operation)
            })
            .map(|id| {
                let operands = resolved.operands(id);
                operands.iter().map(|operand| operand.demand).collect()
            })
            .collect::<Vec<Vec<_>>>()
    };
    let (always, reached) = (Demand::Always, Demand::Reached);
    let assembly = vec![always, reached, reached];
    assert_eq!(demands(".dtc"), [assembly.clone(), assembly]);
    assert_eq!(
        demands(".study_day"),
        [vec![always, always], vec![always, always]]
    );
}

#[test]
fn a_failed_resolution_check_is_an_internal_bind_rejection() {
    use yamaa_adapters::specification_diagnostics::{failure, Rejection};
    use yamaa_application::specification_run::Error;
    use yamaa_core::application_issue::{rejected, Fault, Step};
    use yamaa_core::{dataset::ResolutionError, specification::BindError};
    let reply = failure(
        &Error::Bind(BindError::Resolution(ResolutionError::Capacity)),
        None,
    );
    assert_eq!(
        reply,
        Rejection::Fault(rejected(Step::Bind, Fault::Internal))
    );
}
