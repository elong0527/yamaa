//! Ordered, bounded versionless call selection without code or study effects.
use crate::{
    function_signature::ProjectInvocationPlan,
    project_call_document::Call,
    project_function::{CallArgument, CallFinding, Function},
    value::Value,
};
use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

#[derive(Clone, Debug, PartialEq)]
pub struct LocatedCall {
    pub path: String,
    pub call: Call,
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub definitions: usize,
    pub calls: usize,
    pub arguments: usize,
    pub parameters: usize,
    pub text_bytes: usize,
    pub work: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            definitions: 65_536,
            calls: 1024,
            arguments: 65_536,
            parameters: 65_536,
            text_bytes: 16_777_216,
            work: 67_108_864,
        }
    }
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Limit(&'static str),
    DuplicateDefinitions(Vec<usize>),
    Findings(Vec<Finding>),
    InvalidAdmittedSignature,
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
fn charge(
    used: &mut usize,
    amount: usize,
    limit: usize,
    resource: &'static str,
) -> Result<(), Error> {
    *used = used
        .checked_add(amount)
        .filter(|&n| n <= limit)
        .ok_or(Error::Limit(resource))?;
    Ok(())
}
fn times(a: usize, b: usize, resource: &'static str) -> Result<usize, Error> {
    a.checked_mul(b).ok_or(Error::Limit(resource))
}
fn text_bytes(value: &Value) -> usize {
    match value {
        Value::Str(s) => s.len(),
        _ => 0,
    }
}
impl ProjectCalls {
    /// Definitions retain environment order; calls retain authored occurrence order.
    /// Literal/name/required-argument findings precede activation. Reference kinds
    /// and scope are bound separately against immutable compiler declarations.
    pub fn admit(
        functions: &[Function],
        calls: Vec<LocatedCall>,
        limits: Limits,
    ) -> Result<Self, Error> {
        Self::admit_implementation(functions, calls, None, None, limits)
    }
    /// Reserve compiler-selected multiplicities after earlier complete static validation.
    pub fn admit_with_uses(
        functions: &[Function],
        calls: Vec<LocatedCall>,
        uses: &[usize],
        limits: Limits,
    ) -> Result<Self, Error> {
        Self::admit_implementation(functions, calls, None, Some(uses), limits)
    }
    /// Transfer authored calls to another owned admission phase without cloning them.
    pub fn into_located_calls(self) -> Vec<LocatedCall> {
        self.calls.into_iter().map(|call| call.located).collect()
    }
    /// Known declared reference kinds participate in the same complete argument
    /// pass. Unknown bare outputs are static faults; stored fields may remain
    /// unknown until the compiler binds captured source metadata.
    pub fn admit_with_reference_types(
        functions: &[Function],
        calls: Vec<LocatedCall>,
        reference_types: &BTreeMap<&str, crate::value::ValueType>,
        limits: Limits,
    ) -> Result<Self, Error> {
        Self::admit_implementation(functions, calls, Some(reference_types), None, limits)
    }
    /// Admit each authored call once while reserving all effective row-template
    /// copies. Zero uses still receives complete static validation, but does not
    /// select a function for activation. Uses are compiler-selected metadata.
    pub fn admit_with_reference_types_and_uses(
        functions: &[Function],
        calls: Vec<LocatedCall>,
        reference_types: &BTreeMap<&str, crate::value::ValueType>,
        uses: &[usize],
        limits: Limits,
    ) -> Result<Self, Error> {
        Self::admit_implementation(functions, calls, Some(reference_types), Some(uses), limits)
    }
    fn admit_implementation(
        functions: &[Function],
        calls: Vec<LocatedCall>,
        reference_types: Option<&BTreeMap<&str, crate::value::ValueType>>,
        uses: Option<&[usize]>,
        limits: Limits,
    ) -> Result<Self, Error> {
        if uses.is_some_and(|uses| uses.len() != calls.len()) {
            return Err(Error::InvalidCallUses);
        }
        let use_count = |call: usize| uses.map_or(1, |uses| uses[call]);
        let (mut definition_count, mut call_count) = (0, 0);
        charge(
            &mut definition_count,
            functions.len(),
            limits.definitions,
            "definitions",
        )?;
        charge(&mut call_count, calls.len(), limits.calls, "calls")?;
        let mut bound_calls = 0;
        for call in 0..calls.len() {
            use_count(call)
                .checked_add(1)
                .ok_or(Error::Limit("bound_calls"))?;
            charge(
                &mut bound_calls,
                use_count(call),
                limits.calls,
                "bound_calls",
            )?;
        }

        let (mut text, mut argument_count) = (0, 0);
        for function in functions {
            charge(
                &mut text,
                function.definition().name.len(),
                limits.text_bytes,
                "text",
            )?;
        }
        for (call, located) in calls.iter().enumerate() {
            charge(&mut text, located.path.len(), limits.text_bytes, "text")?;
            charge(
                &mut text,
                located.call.name.len(),
                limits.text_bytes,
                "text",
            )?;
            charge(
                &mut argument_count,
                located.call.arguments.len(),
                limits.arguments,
                "arguments",
            )?;
            for argument in &located.call.arguments {
                charge(
                    &mut text,
                    times(argument.name.len(), (1 + use_count(call)).max(2), "text")?,
                    limits.text_bytes,
                    "text",
                )?;
                let bytes = match &argument.input {
                    crate::project_call_document::Input::Reference(name) => name.len(),
                    crate::project_call_document::Input::Literal(value) => text_bytes(value),
                };
                charge(
                    &mut text,
                    times(bytes, 1 + use_count(call), "text")?,
                    limits.text_bytes,
                    "text",
                )?;
            }
        }
        let mut bound_arguments = 0;
        for (call, located) in calls.iter().enumerate() {
            charge(
                &mut bound_arguments,
                times(
                    located.call.arguments.len(),
                    use_count(call),
                    "bound_arguments",
                )?,
                limits.arguments,
                "bound_arguments",
            )?;
        }
        // Borrow names after aggregate admission; no string copies or function
        // plans are made until all anticipated per-call ownership is bounded.
        let mut work = 0;
        // Bound ordered name indexing and all three lookup passes, including
        // long shared prefixes. B-tree node searches compare a bounded number
        // of keys per level; charge sixteen comparisons per binary depth.
        let depth = usize::BITS as usize - functions.len().max(1).leading_zeros() as usize;
        let comparisons = times(depth + 1, 16, "work")?;
        for function in functions {
            charge(
                &mut work,
                times(function.definition().name.len().max(1), comparisons, "work")?,
                limits.work,
                "work",
            )?;
        }
        for located in &calls {
            let search = times(located.call.name.len().max(1), comparisons, "work")?;
            charge(&mut work, times(search, 3, "work")?, limits.work, "work")?;
        }
        let mut index = BTreeMap::new();
        let mut duplicates = Vec::new();
        for (i, function) in functions.iter().enumerate() {
            if index
                .insert(function.definition().name.as_str(), i)
                .is_some()
            {
                duplicates.push(i);
            }
        }
        if !duplicates.is_empty() {
            return Err(Error::DuplicateDefinitions(duplicates));
        }
        let mut parameters = 0;
        let mut selected = vec![false; functions.len()];
        for (call, located) in calls.iter().enumerate() {
            let Some(&i) = index.get(located.call.name.as_str()) else {
                continue;
            };
            let d = functions[i].definition();
            // One retained unique plan plus every eventual bound-expression copy.
            // Fully overridden defaults remain static calls without owned plans.
            let copies = use_count(call) + usize::from(use_count(call) != 0 && !selected[i]);
            charge(
                &mut parameters,
                times(d.params.len(), copies, "parameters")?,
                limits.parameters,
                "parameters",
            )?;
            let identity = d
                .name
                .len()
                .checked_add(d.function.len())
                .ok_or(Error::Limit("text"))?;
            charge(
                &mut text,
                times(identity, copies, "text")?,
                limits.text_bytes,
                "text",
            )?;
            let mut max_name = 1;
            for p in &d.params {
                max_name = max_name.max(p.name.len());
                let bytes = p
                    .name
                    .len()
                    .checked_mul(2)
                    .and_then(|n| n.checked_add(p.default.as_ref().map_or(0, text_bytes)))
                    .ok_or(Error::Limit("text"))?;
                charge(
                    &mut text,
                    times(bytes, copies, "text")?,
                    limits.text_bytes,
                    "text",
                )?;
            }
            // Projected findings repeat the logical identity and authored path.
            // Reserve all independent argument and missing-required diagnostics
            // before building even the lightweight indexed finding vector.
            let potential = located
                .call
                .arguments
                .len()
                .checked_mul(2)
                .and_then(|n| n.checked_add(d.params.len()))
                .and_then(|n| n.checked_add(1))
                .ok_or(Error::Limit("text"))?;
            let max_argument = located
                .call
                .arguments
                .iter()
                .map(|a| a.name.len())
                .chain(d.params.iter().map(|p| p.name.len()))
                .max()
                .unwrap_or(0);
            let finding_bytes = d
                .name
                .len()
                .checked_add(located.path.len())
                .and_then(|n| n.checked_add(max_argument.checked_mul(2)?))
                .and_then(|n| n.checked_add(16))
                .ok_or(Error::Limit("text"))?;
            charge(
                &mut text,
                times(
                    times(finding_bytes, potential, "text")?,
                    use_count(call).max(1),
                    "text",
                )?,
                limits.text_bytes,
                "text",
            )?;
            // Complete named matching, including missing-required checks and
            // long shared prefixes, has a cumulative semantic-work ceiling.
            let searches = located
                .call
                .arguments
                .len()
                .checked_add(1)
                .ok_or(Error::Limit("work"))?;
            charge(
                &mut work,
                times(
                    times(times(searches, d.params.len(), "work")?, max_name, "work")?,
                    1 + use_count(call),
                    "work",
                )?,
                limits.work,
                "work",
            )?;
            selected[i] |= use_count(call) != 0;
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
