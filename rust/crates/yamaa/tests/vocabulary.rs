//! A diagnostic's phase and condition are the registry's vocabulary (#3765):
//! a [`Phase`] is one of the phases `yaml/conditions.yaml` names, and a
//! [`ConditionCode`] a condition it holds, registered or retired. Both are
//! derived here from the registry file itself, and both refuse what the
//! registry lacks, a condition's former name included.

use std::collections::BTreeSet;

use yamaa::{ConditionCode, Phase, PHASES};

/// The registry as `yaml/conditions.yaml` writes it.
#[derive(Default)]
struct Registry {
    /// Each registered condition with its phases, in file order.
    registered: Vec<(&'static str, Vec<&'static str>)>,
    /// Each name a release renamed a registered condition from.
    former: Vec<&'static str>,
    /// Each condition a release retired.
    retired: Vec<&'static str>,
}

/// The registry's two sections, read line by line: a condition is a key two
/// spaces deep, its phases a one-line list and its former name a value four
/// spaces deep.
fn registry() -> Registry {
    let mut registry = Registry::default();
    let mut section = "";
    for line in include_str!("../../../../yaml/conditions.yaml").lines() {
        if line.trim_start().starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            section = line;
        } else if let Some(name) = line
            .strip_prefix("  ")
            .and_then(|rest| rest.strip_suffix(':'))
            .filter(|name| !name.starts_with(' '))
        {
            match section {
                "conditions:" => registry.registered.push((name, Vec::new())),
                "retired:" => registry.retired.push(name),
                other => panic!("a condition outside the two sections: {other}"),
            }
        } else if let Some(phases) = line
            .strip_prefix("    phases: [")
            .and_then(|rest| rest.strip_suffix(']'))
        {
            let (name, listed) = registry.registered.last_mut().expect("a condition");
            assert!(listed.is_empty(), "{name} lists its phases twice");
            *listed = phases.split(", ").collect();
        } else if let Some(former) = line.strip_prefix("    renamed_from: ") {
            registry.former.push(former);
        }
    }
    registry
}

#[test]
fn every_registered_and_retired_condition_and_every_phase_is_typed() {
    let registry = registry();
    assert!(
        registry.registered.len() > 200 && registry.retired.len() > 5,
        "the registry was read whole"
    );
    for (name, phases) in &registry.registered {
        assert!(!phases.is_empty(), "{name} lists its phases");
        assert_eq!(
            (
                ConditionCode::parse(name).map(ConditionCode::as_str),
                phases
                    .iter()
                    .map(|phase| Phase::parse(phase).map(Phase::as_str))
                    .collect::<Vec<_>>(),
            ),
            (
                Some(*name),
                phases.iter().map(|phase| Some(*phase)).collect()
            ),
            "{name}"
        );
    }
    for name in &registry.retired {
        assert_eq!(
            ConditionCode::parse(name).map(ConditionCode::as_str),
            Some(*name),
            "retired {name}"
        );
    }
}

/// The phases the registry names are the twelve, which `PHASES` lists in run
/// order.
#[test]
fn the_registry_names_the_twelve_phases() {
    let named = registry()
        .registered
        .into_iter()
        .flat_map(|(_, phases)| phases)
        .collect::<BTreeSet<_>>();
    assert_eq!(named, PHASES.into_iter().collect::<BTreeSet<_>>());
}

/// What the registry lacks is no phase and no condition: another phase name,
/// a spelling that differs only in case, an unknown name, and each former
/// name a release renamed, which a program written for that release may
/// still match.
#[test]
fn a_phase_or_condition_the_registry_lacks_is_refused() {
    for phase in ["staging", "Validation", "row-construction", ""] {
        assert_eq!(Phase::parse(phase), None, "phase {phase:?}");
    }
    let registry = registry();
    assert!(
        !registry.former.is_empty(),
        "the registry renames conditions"
    );
    for name in ["not_a_condition", "Unknown_Field", ""]
        .into_iter()
        .chain(registry.former)
    {
        assert_eq!(ConditionCode::parse(name), None, "condition {name:?}");
    }
}
