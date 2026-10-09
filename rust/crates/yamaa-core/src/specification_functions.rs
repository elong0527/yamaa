//! Versionless call preparation and retained diagnostic projection.
use super::*;
use crate::{
    diagnostic::{ConditionCode as C, ContextValue as V, Diagnostic},
    project_call_document::{self, Input},
    project_calls::{self, LocatedCall, ProjectCalls},
    project_function::{CallFinding, Function},
    value::{Value, ValueType},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cause {
    UnknownFunction,
    UnknownReference(String),
    RowPhase {
        identifier: String,
        row: String,
    },
    UnknownArgument,
    DuplicateArgument,
    MissingRequiredArgument,
    ArgumentType {
        expected: ValueType,
        actual: ValueType,
    },
    Scalar(crate::project_function_document::Kind),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionFinding {
    pub path: String,
    pub function: String,
    pub argument: Option<String>,
    pub node: usize,
    pub cause: Cause,
}
fn type_name(kind: ValueType) -> &'static str {
    match kind {
        ValueType::Str => "str",
        ValueType::Int => "int",
        ValueType::Float => "float",
        ValueType::Bool => "bool",
        ValueType::Date => "date",
        ValueType::DateTime => "datetime",
    }
}
impl FunctionFinding {
    pub fn diagnostic(&self) -> Diagnostic {
        let mut context = crate::diagnostic::Context::new();
        let text = |s: &str| V::Scalar(Value::Str(s.into()));
        context.insert("function".into(), text(&self.function));
        if let Some(argument) = &self.argument {
            context.insert("argument".into(), text(argument));
        }
        let (code, cause) = match self.cause {
            Cause::UnknownReference(ref identifier) => {
                context.insert("identifier".into(), text(identifier));
                (C::OutputUnknownReference, None)
            }
            Cause::RowPhase {
                ref identifier,
                ref row,
            } => {
                context.insert("identifier".into(), text(identifier));
                context.insert("row".into(), text(row));
                context.insert("available_phase".into(), text("column_derivation"));
                context.insert("required_phase".into(), text("row_construction"));
                (C::RowDependencyPhaseBoundary, None)
            }
            Cause::UnknownFunction => (C::UnknownProjectFunction, None),
            Cause::UnknownArgument => (C::InvalidFunctionArgument, Some("unknown_argument")),
            Cause::DuplicateArgument => (C::InvalidFunctionArgument, Some("duplicate_argument")),
            Cause::MissingRequiredArgument => (
                C::InvalidFunctionArgument,
                Some("missing_required_argument"),
            ),
            Cause::ArgumentType { expected, actual } => {
                context.insert("expected_type".into(), text(type_name(expected)));
                context.insert("actual_type".into(), text(type_name(actual)));
                (C::InvalidFunctionArgument, Some("type_mismatch"))
            }
            Cause::Scalar(kind) => (
                C::InvalidFunctionArgument,
                Some(match kind {
                    crate::project_function_document::Kind::NormalizedShape => "invalid_shape",
                    crate::project_function_document::Kind::IntegerRange => "integer_range",
                    crate::project_function_document::Kind::InvalidDate => "invalid_date",
                    crate::project_function_document::Kind::InvalidDateTime => "invalid_datetime",
                }),
            ),
        };
        if let Some(cause) = cause {
            context.insert("cause".into(), text(cause));
        }
        Diagnostic {
            code,
            spec_paths: vec![self.path.clone()],
            context,
            source_span: None,
            operand_route: None,
        }
    }
}

pub(super) struct Prepared {
    pub calls: ProjectCalls,
    pub nodes: BTreeMap<usize, usize>,
}
fn invalid(finding: FunctionFinding) -> PreflightFinding {
    PreflightFinding::ProjectFunction(finding)
}

pub(super) fn prepare(
    d: &Document,
    columns: &[usize],
    functions: &[Function],
) -> Result<Prepared, PrepareError> {
    // The held environment is globally admitted. Bound a borrowed lookup index
    // for malformed-call name checks and missing-parameter projection too;
    // neither path may rescan all definitions once per retained diagnostic.
    if functions.len() > 65_536 {
        return Err(PrepareError::Limit("function_definitions"));
    }
    let depth = usize::BITS as usize - functions.len().max(1).leading_zeros() as usize;
    let mut name_bytes = 0usize;
    for function in functions {
        name_bytes = name_bytes
            .checked_add(function.definition().name.len())
            .filter(|&n| n <= 16_777_216)
            .ok_or(PrepareError::Limit("function_definitions"))?;
    }
    if name_bytes
        .checked_mul((depth + 1) * 16)
        .is_none_or(|n| n > 67_108_864)
    {
        return Err(PrepareError::Limit("function_definitions"));
    }
    let mut function_index = BTreeMap::new();
    for function in functions {
        if function_index
            .insert(function.definition().name.as_str(), function)
            .is_some()
        {
            return Err(PrepareError::Internal);
        }
    }
    let mut requests = Vec::new();
    for &column in columns {
        if let Some(node) = d
            .field(column, "derivation")
            .and_then(|id| d.field(id, "value"))
            .and_then(|id| d.field(id, "function"))
        {
            let name = text(d, field(d, column, "name")?)?;
            requests.push((node, format!("columns.{name}.derivation.function")));
        }
    }
    let has_rows = present(d, d.root(), "rows");
    if has_rows {
        let rows = sequence(d, field(d, d.root(), "rows")?)?;
        if rows.len() > 16 {
            return Err(PrepareError::Limit("row_templates"));
        }
        for (row, &node) in rows.iter().enumerate() {
            for &(name, derivation) in mapping(d, field(d, node, "derivations")?)? {
                if let Some(node) = d
                    .field(derivation, "value")
                    .and_then(|id| d.field(id, "function"))
                {
                    if requests.len() >= 1024 {
                        return Err(PrepareError::Limit("function_calls"));
                    }
                    requests.push((
                        node,
                        format!("rows[{row}].derivations.{}.function", text(d, name)?),
                    ));
                }
            }
        }
    }
    let mut bytes = 0usize;
    for (node, path) in &requests {
        let cost = project_call_document::text_cost(d, *node, path, Default::default())
            .map_err(|_| PrepareError::Limit("function_calls"))?;
        bytes = bytes
            .checked_add(cost)
            .filter(|&n| n <= 16_777_216)
            .ok_or(PrepareError::Limit("function_calls"))?;
    }
    let mut calls = Vec::new();
    let mut origins = Vec::new();
    let mut findings = Vec::new();
    for (request, (node, path)) in requests.iter().enumerate() {
        match project_call_document::decode(d, *node, path, Default::default()) {
            Ok(call) => {
                origins.push((request, *node, path.clone(), call.name.clone()));
                calls.push(LocatedCall {
                    path: path.clone(),
                    call,
                });
            }
            Err(project_call_document::Error::Findings(errors)) => {
                let function = d
                    .field(*node, "name")
                    .and_then(|id| match &d.nodes()[id] {
                        N::Text(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("");
                if !function.is_empty() && !function_index.contains_key(function) {
                    findings.push((
                        request,
                        invalid(FunctionFinding {
                            path: format!("{path}.name"),
                            function: function.into(),
                            argument: None,
                            node: *node,
                            cause: Cause::UnknownFunction,
                        }),
                    ));
                }
                findings.extend(errors.into_iter().map(|error| {
                    (
                        request,
                        invalid(FunctionFinding {
                            path: error.path,
                            function: function.into(),
                            argument: None,
                            node: error.node,
                            cause: Cause::Scalar(error.kind),
                        }),
                    )
                }));
            }
            Err(project_call_document::Error::Limit(_)) => {
                return Err(PrepareError::Limit("function_calls"))
            }
        }
    }
    // Hold only source geometry for failure projection; selection consumes calls.
    let argument_origins = calls
        .iter()
        .map(|c| {
            c.call
                .arguments
                .iter()
                .map(|a| {
                    (
                        a.name.clone(),
                        a.node,
                        match &a.input {
                            Input::Reference(name) => Some(name.clone()),
                            _ => None,
                        },
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let types = columns
        .iter()
        .map(|&id| {
            let name = text(d, field(d, id, "name")?)?;
            let kind = match text(d, field(d, id, "type")?)? {
                "str" => ValueType::Str,
                "int" => ValueType::Int,
                "float" => ValueType::Float,
                "date" => ValueType::Date,
                "datetime" => ValueType::DateTime,
                _ => return Err(PrepareError::Internal),
            };
            Ok((name, kind))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let definitions = origins
        .iter()
        .map(|(_, _, _, name)| function_index.get(name.as_str()).copied())
        .collect::<Vec<_>>();
    let uses = alloc::vec![usize::from(!has_rows); calls.len()];
    let admitted = match ProjectCalls::admit_with_reference_types_and_uses(
        functions,
        calls,
        &types,
        &uses,
        Default::default(),
    ) {
        Ok(calls) => Some(calls),
        Err(project_calls::Error::Limit(_)) => return Err(PrepareError::Limit("function_calls")),
        Err(
            project_calls::Error::DuplicateDefinitions(_)
            | project_calls::Error::InvalidAdmittedSignature
            | project_calls::Error::InvalidCallUses,
        ) => return Err(PrepareError::Internal),
        Err(project_calls::Error::Findings(errors)) => {
            for error in errors {
                let (origin, node, path, function) = &origins[error.call];
                let arguments = &argument_origins[error.call];
                let (cause, argument, node, path) = match error.kind {
                    project_calls::Kind::UnknownFunction => {
                        (Cause::UnknownFunction, None, *node, format!("{path}.name"))
                    }
                    project_calls::Kind::UnknownReference { argument } => {
                        let (name, node, reference) = &arguments[argument];
                        (
                            Cause::UnknownReference(
                                reference.clone().ok_or(PrepareError::Internal)?,
                            ),
                            Some(name.clone()),
                            *node,
                            format!("{path}.args.{name}"),
                        )
                    }
                    project_calls::Kind::Argument(finding) => {
                        let (index, cause) = match finding {
                            CallFinding::UnknownArgument { argument } => {
                                (Some(argument), Cause::UnknownArgument)
                            }
                            CallFinding::DuplicateArgument { argument } => {
                                (Some(argument), Cause::DuplicateArgument)
                            }
                            CallFinding::ArgumentType {
                                argument,
                                expected,
                                actual,
                            } => (Some(argument), Cause::ArgumentType { expected, actual }),
                            CallFinding::MissingRequiredArgument { parameter } => {
                                let f = definitions[error.call].ok_or(PrepareError::Internal)?;
                                let argument = f
                                    .definition()
                                    .params
                                    .get(parameter)
                                    .ok_or(PrepareError::Internal)?
                                    .name
                                    .clone();
                                findings.push((
                                    *origin,
                                    invalid(FunctionFinding {
                                        path: format!("{path}.args"),
                                        function: function.clone(),
                                        argument: Some(argument),
                                        node: *node,
                                        cause: Cause::MissingRequiredArgument,
                                    }),
                                ));
                                continue;
                            }
                            CallFinding::Limit(_) => {
                                return Err(PrepareError::Limit("function_calls"))
                            }
                        };
                        let (argument, value, _) =
                            &arguments[index.ok_or(PrepareError::Internal)?];
                        (
                            cause,
                            Some(argument.clone()),
                            *value,
                            format!("{path}.args.{argument}"),
                        )
                    }
                };
                findings.push((
                    *origin,
                    invalid(FunctionFinding {
                        path,
                        function: function.clone(),
                        argument,
                        node,
                        cause,
                    }),
                ));
            }
            None
        }
    };
    if !findings.is_empty() {
        findings.sort_by_key(|(origin, _)| *origin);
        return Err(PrepareError::Invalid(
            findings.into_iter().map(|(_, finding)| finding).collect(),
        ));
    }
    let calls = admitted.ok_or(PrepareError::Internal)?;
    let nodes = origins
        .into_iter()
        .enumerate()
        .map(|(index, (_, node, _, _))| (node, index))
        .collect();
    Ok(Prepared { calls, nodes })
}

/// Static row calls remain unselected until defaults and per-template overrides
/// have established their actual multiplicities. Move calls rather than clone them.
pub(super) fn select_rows(
    prepared: Prepared,
    functions: &[Function],
    uses: &[usize],
) -> Result<Prepared, PrepareError> {
    let calls = ProjectCalls::admit_with_uses(
        functions,
        prepared.calls.into_located_calls(),
        uses,
        Default::default(),
    )
    .map_err(|error| match error {
        project_calls::Error::Limit(_) => PrepareError::Limit("function_calls"),
        _ => PrepareError::Internal,
    })?;
    Ok(Prepared {
        calls,
        nodes: prepared.nodes,
    })
}

pub(super) fn bind_findings(
    calls: &ProjectCalls,
    call: usize,
    errors: Vec<crate::project_call_binding::Finding>,
    row: Option<&str>,
) -> Result<Vec<BindFinding>, BindError> {
    let call = &calls.calls().get(call).ok_or(BindError::Internal)?.located;
    errors
        .into_iter()
        .map(|error| {
            let argument = call
                .call
                .arguments
                .get(error.argument)
                .ok_or(BindError::Internal)?;
            let path = format!("{}.args.{}", call.path, argument.name);
            let Input::Reference(name) = &argument.input else {
                return Err(BindError::Internal);
            };
            Ok(match error.kind {
                crate::project_call_binding::Kind::Output(
                    crate::reference_binding::Diagnostic::PhaseBoundary { .. },
                ) => BindFinding::ProjectFunction(FunctionFinding {
                    path,
                    function: call.call.name.clone(),
                    argument: Some(argument.name.clone()),
                    node: argument.node,
                    cause: Cause::RowPhase {
                        identifier: name.clone(),
                        row: row.ok_or(BindError::Internal)?.into(),
                    },
                }),
                crate::project_call_binding::Kind::Output(finding) => {
                    BindFinding::OutputReference {
                        path,
                        name: name.clone(),
                        finding,
                    }
                }
                crate::project_call_binding::Kind::Scope(
                    crate::reference_scope::Finding::DriverMismatch
                    | crate::reference_scope::Finding::UnknownField,
                ) => BindFinding::UnknownReference {
                    path,
                    name: name.clone(),
                },
                crate::project_call_binding::Kind::Scope(finding) => {
                    BindFinding::QualifiedReference {
                        path,
                        name: name.clone(),
                        row: row.map(String::from),
                        finding,
                    }
                }
                crate::project_call_binding::Kind::ArgumentType { expected, actual } => {
                    BindFinding::ProjectFunction(FunctionFinding {
                        path,
                        function: call.call.name.clone(),
                        argument: Some(argument.name.clone()),
                        node: argument.node,
                        cause: Cause::ArgumentType { expected, actual },
                    })
                }
                crate::project_call_binding::Kind::UnsupportedRelation { .. } => {
                    return Err(BindError::Internal)
                }
            })
        })
        .collect()
}
