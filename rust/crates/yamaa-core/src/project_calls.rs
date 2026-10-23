//! Ordered versionless call selection without code or study effects.
use crate::{
    function_signature::ProjectInvocationPlan,
    project_call_document::Call,
    project_function::{CallArgument, CallFinding, Function},
};
use std::{collections::BTreeMap, vec};

/// One authored call: its own spec path, the spec path of the derivation
/// that evaluates it, that derivation's position in authored order, and the
/// call as written.
#[derive(Clone, Debug, PartialEq)]
pub struct LocatedCall {
    pub path: String,
    pub derivation: String,
    /// Authored order is the order a formatted file writes derivations in:
    /// the intermediates', then the columns', then the row templates', each
    /// in declaration order (D221).
    pub authored: usize,
    pub call: Call,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    UnknownFunction,
    UnknownReference { argument: usize },
    Argument(CallFinding),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub call: usize,
    pub kind: Kind,
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("project calls duplicate definitions: {0:?}")]
    DuplicateDefinitions(Vec<usize>),
    #[error("project calls findings: {0:?}")]
    Findings(Vec<Finding>),
    #[error("project calls invalid admitted signature")]
    InvalidAdmittedSignature,
    #[error("project calls invalid call uses")]
    InvalidCallUses,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedCall {
    pub located: LocatedCall,
    slot: Option<usize>,
}
impl PreparedCall {
    /// A statically valid but fully overridden default has no activation slot.
    pub fn slot(&self) -> Option<usize> {
        self.slot
    }
}
#[derive(Debug)]
pub struct ProjectCalls {
    selected: Vec<usize>,
    plans: Vec<ProjectInvocationPlan>,
    calls: Vec<PreparedCall>,
}
impl ProjectCalls {
    /// Definitions retain environment order; calls retain authored occurrence order.
    /// Literal/name/required-argument findings precede activation. Reference kinds
    /// and scope are bound separately against immutable compiler declarations.
    pub fn admit(functions: &[Function], calls: Vec<LocatedCall>) -> Result<Self, Error> {
        Self::admit_implementation(functions, calls, None, None, None)
    }
    /// Use compiler-selected multiplicities after earlier complete static validation.
    pub fn admit_with_uses(
        functions: &[Function],
        calls: Vec<LocatedCall>,
        uses: &[usize],
    ) -> Result<Self, Error> {
        Self::admit_implementation(functions, calls, None, Some(uses), None)
    }
    /// Transfer authored calls to another owned admission phase without cloning them.
    pub fn into_located_calls(self) -> Vec<LocatedCall> {
        self.calls.into_iter().map(|call| call.located).collect()
    }
    /// Admit each authored call once with its effective row-template uses.
    /// Zero uses still receives complete static validation, but does not
    /// select a function for activation. Uses are compiler-selected metadata.
    pub fn admit_with_reference_types_and_uses(
        functions: &[Function],
        calls: Vec<LocatedCall>,
        reference_types: &BTreeMap<&str, crate::value::ValueType>,
        uses: &[usize],
    ) -> Result<Self, Error> {
        Self::admit_implementation(functions, calls, Some(reference_types), Some(uses), None)
    }
    /// Record declarations bind reference types in their actual donor catalogue.
    pub(crate) fn admit_contextual(
        functions: &[Function],
        calls: Vec<LocatedCall>,
        types: &BTreeMap<&str, crate::value::ValueType>,
        uses: &[usize],
        contextual: &[bool],
    ) -> Result<Self, Error> {
        Self::admit_implementation(functions, calls, Some(types), Some(uses), Some(contextual))
    }
    fn admit_implementation(
        functions: &[Function],
        calls: Vec<LocatedCall>,
        reference_types: Option<&BTreeMap<&str, crate::value::ValueType>>,
        uses: Option<&[usize]>,
        contextual: Option<&[bool]>,
    ) -> Result<Self, Error> {
        if contextual.is_some_and(|contextual| contextual.len() != calls.len())
            || uses.is_some_and(|uses| uses.len() != calls.len())
        {
            return Err(Error::InvalidCallUses);
        }
        let use_count = |call: usize| uses.map_or(1, |uses| uses[call]);
        let mut index = BTreeMap::new();
        let mut duplicates = Vec::new();
        for (i, function) in functions.iter().enumerate() {
            if index.insert(function.name(), i).is_some() {
                duplicates.push(i);
            }
        }
        if !duplicates.is_empty() {
            return Err(Error::DuplicateDefinitions(duplicates));
        }
        let mut selected = vec![false; functions.len()];
        for (call, located) in calls.iter().enumerate() {
            if let Some(&i) = index.get(located.call.name.as_str()) {
                selected[i] |= use_count(call) != 0;
            }
        }
        let mut findings = Vec::new();
        for (call, located) in calls.iter().enumerate() {
            let Some(&i) = index.get(located.call.name.as_str()) else {
                findings.push(Finding {
                    call,
                    kind: Kind::UnknownFunction,
                });
                continue;
            };
            let reference_types = if contextual.is_some_and(|contextual| contextual[call]) {
                None
            } else {
                reference_types
            };
            if let Some(types) = reference_types {
                for (argument, item) in located.call.arguments.iter().enumerate() {
                    if let crate::project_call_document::Input::Reference(name) = &item.input {
                        if !name.contains('.') && !types.contains_key(name.as_str()) {
                            findings.push(Finding {
                                call,
                                kind: Kind::UnknownReference { argument },
                            });
                        }
                    }
                }
            }
            let arguments = located
                .call
                .arguments
                .iter()
                .map(|a| CallArgument {
                    name: a.name.clone(),
                    kind: match &a.input {
                        crate::project_call_document::Input::Literal(v) => v.value_type(),
                        crate::project_call_document::Input::Reference(name) => {
                            reference_types.and_then(|types| types.get(name.as_str()).copied())
                        }
                    },
                })
                .collect::<Vec<_>>();
            findings.extend(
                functions[i]
                    .bind_call(&arguments)
                    .into_iter()
                    .map(|finding| Finding {
                        call,
                        kind: Kind::Argument(finding),
                    }),
            );
        }
        if !findings.is_empty() {
            return Err(Error::Findings(findings));
        }
        let selected = selected
            .into_iter()
            .enumerate()
            .filter_map(|(i, used)| used.then_some(i))
            .collect::<Vec<_>>();
        let plans = selected
            .iter()
            .map(|&i| {
                functions[i]
                    .invocation_plan()
                    .map_err(|_| Error::InvalidAdmittedSignature)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let slots = selected
            .iter()
            .enumerate()
            .map(|(slot, &i)| (i, slot))
            .collect::<BTreeMap<_, _>>();
        let calls = calls
            .into_iter()
            .enumerate()
            .map(|(call, located)| {
                let i = index[located.call.name.as_str()];
                PreparedCall {
                    located,
                    slot: (use_count(call) != 0).then(|| slots[&i]),
                }
            })
            .collect();
        Ok(Self {
            selected,
            plans,
            calls,
        })
    }
    pub fn selected(&self) -> &[usize] {
        &self.selected
    }
    pub fn plans(&self) -> &[ProjectInvocationPlan] {
        &self.plans
    }
    pub fn calls(&self) -> &[PreparedCall] {
        &self.calls
    }
}
