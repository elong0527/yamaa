//! Versionless call preparation and retained diagnostic projection.
use super::*;
use crate::json::Value as V;
use crate::{
    diagnostic::{ConditionCode as C, Diagnostic},
    project_call_document::{self, Input},
    project_calls::{self, LocatedCall, ProjectCalls},
    project_function::{CallFinding, Function},
    value::ValueType,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cause {
    UnknownFunction,
    /// The supplied project links no environment file for the host language.
    UndeclaredEnvironment(crate::project_function::Language),
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
        let text = |s: &str| V::String(s.into());
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
            Cause::UndeclaredEnvironment(language) => {
                context.insert("language".into(), text(language.key()));
                (C::ProjectEnvironmentMissing, Some("undeclared"))
            }
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
    pub returns: BTreeMap<usize, ColumnType>,
    /// The calls each derivation evaluates, by its operation's document node.
    sites: BTreeMap<usize, Vec<usize>>,
    /// The calls intermediate derivations evaluate.
    bindings: Vec<usize>,
}
impl Prepared {
    /// The calls of the derivation whose operation is the document node `value`.
    pub(super) fn site_calls(&self, value: usize) -> &[usize] {
        self.sites.get(&value).map_or(&[], Vec::as_slice)
    }
    /// Each call's uses in a spec with row templates: one for each row
    /// declaration (by its operation node) that evaluates it, and one for
    /// each intermediate derivation.
    pub(super) fn row_uses(
        &self,
        declarations: impl Iterator<Item = usize>,
    ) -> Result<Vec<usize>, PrepareError> {
        let mut uses = std::vec![0usize; self.calls.calls().len()];
        for &call in declarations
            .flat_map(|value| self.site_calls(value))
            .chain(&self.bindings)
        {
            let count = uses.get_mut(call).ok_or(PrepareError::Internal)?;
            *count = count.checked_add(1).ok_or(PrepareError::Internal)?;
        }
        Ok(uses)
    }
}
fn invalid(finding: FunctionFinding) -> PreflightFinding {
    PreflightFinding::ProjectFunction(finding)
}

/// One call a derivation evaluates: its payload node, path and whether its
/// references read a record context (an aggregate binding or a donor).
struct Request {
    node: usize,
    path: String,
    contextual: bool,
}

/// Every call the derivations evaluate, in today's occurrence order:
/// columns, then intermediate derivations, then row template overrides.
fn requests(
    d: &Document,
    lowered: &crate::front_end::v0_5::Lowered,
) -> (Vec<Request>, BTreeMap<usize, Vec<usize>>, Vec<usize>) {
    use crate::ast::{ExprKind, Notation, Owner, Scope, Step};
    let spec = lowered.spec();
    let sites = spec.sites();
    let mut requests = Vec::new();
    let mut by_site = BTreeMap::new();
    let mut bindings = Vec::new();
    let passes: [fn(&Owner) -> bool; 3] = [
        |owner| matches!(owner, Owner::Default { .. }),
        |owner| matches!(owner, Owner::Binding { .. }),
        |owner| matches!(owner, Owner::Override { .. }),
    ];
    for pass in passes {
        for site in sites.iter().filter(|site| pass(&site.owner)) {
            let mut found = Vec::new();
            spec.walk(site.root, site.scope, |at, scope| {
                if matches!(spec.expr(at).kind, ExprKind::Call { .. }) {
                    found.push((at, scope != Scope::Row));
                }
                Step::Continue
            });
            let mut indices = Vec::new();
            for (call, contextual) in found {
                let Some(node) = d.field(lowered.origin(call), "function") else {
                    continue;
                };
                indices.push(requests.len());
                requests.push(Request {
                    node,
                    path: spec.render(spec.expr(call).meta.path, Notation::V0_5),
                    contextual,
                });
            }
            if matches!(site.owner, Owner::Binding { .. }) {
                bindings.extend(&indices);
            }
            by_site.insert(lowered.origin(site.root), indices);
        }
    }
    (requests, by_site, bindings)
}

pub(super) fn prepare(
    d: &Document,
    lowered: &crate::front_end::v0_5::Lowered,
    columns: &[usize],
    functions: &[Function],
    undeclared: Option<crate::project_function::Language>,
) -> Result<Prepared, PrepareError> {
    // Without a host environment no call names a declared function; the absent
    // environment, not the function name, is the cause each call reports.
    let unknown = || match undeclared {
        Some(language) => Cause::UndeclaredEnvironment(language),
        None => Cause::UnknownFunction,
    };
    let mut function_index = BTreeMap::new();
    for function in functions {
        if function_index.insert(function.name(), function).is_some() {
            return Err(PrepareError::Internal);
        }
    }
    let (requests, sites, bindings) = requests(d, lowered);
    let has_rows = present(d, d.root(), "rows");
    let mut calls = Vec::new();
    let mut origins = Vec::new();
    let mut findings = Vec::new();
    for (request, Request { node, path, .. }) in requests.iter().enumerate() {
        match project_call_document::decode(d, *node, path) {
            Ok(call) => {
                origins.push((request, *node, path.clone(), call.name.clone()));
                calls.push(LocatedCall {
                    path: path.clone(),
                    call,
                });
            }
            Err(errors) => {
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
                            cause: unknown(),
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
    let uses = std::vec![usize::from(!has_rows); calls.len()];
    let contextual = origins
        .iter()
        .map(|(request, _, _, _)| requests[*request].contextual)
        .collect::<Vec<_>>();
    let admitted =
        match ProjectCalls::admit_contextual(functions, calls, &types, &uses, &contextual) {
            Ok(calls) => Some(calls),
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
                            (unknown(), None, *node, format!("{path}.name"))
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
                                    let f =
                                        definitions[error.call].ok_or(PrepareError::Internal)?;
                                    let argument = f
                                        .definition()
                                        .params
                                        .get(parameter)
                                        .ok_or(PrepareError::Internal)?
                                        .name
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
    let returns = origins
        .iter()
        .enumerate()
        .map(|(index, (_, node, _, _))| {
            Ok((
                *node,
                definitions[index]
                    .ok_or(PrepareError::Internal)?
                    .definition()
                    .returns,
            ))
        })
        .collect::<Result<_, PrepareError>>()?;
    let nodes = origins
        .into_iter()
        .enumerate()
        .map(|(index, (_, node, _, _))| (node, index))
        .collect();
    Ok(Prepared {
        calls,
        nodes,
        returns,
        sites,
        bindings,
    })
}

/// Static row calls remain unselected until defaults and per-template overrides
/// have established their actual multiplicities. Move calls rather than clone them.
pub(super) fn select_rows(
    prepared: Prepared,
    functions: &[Function],
    uses: &[usize],
) -> Result<Prepared, PrepareError> {
    let calls = ProjectCalls::admit_with_uses(functions, prepared.calls.into_located_calls(), uses)
        .map_err(|_| PrepareError::Internal)?;
    Ok(Prepared { calls, ..prepared })
}

/// A project function that an intermediate derivation calls is invoked once
/// per donor record, so an argument reads a named intermediate only when it
/// declares `keep`, as an aggregate's derive binding does (LOOKUP-22). The
/// declarations decide it: `named` holds the intermediates declared before the
/// reading one, which are the ones its derivations may read.
pub(super) fn donor_keep_findings(
    project: &Prepared,
    value: usize,
    named: &intermediates::Declarations,
) -> Result<Vec<PreflightFinding>, PrepareError> {
    let mut findings = Vec::new();
    for &call in project.site_calls(value) {
        let located = &project
            .calls
            .calls()
            .get(call)
            .ok_or(PrepareError::Internal)?
            .located;
        for argument in &located.call.arguments {
            let Input::Reference(name) = &argument.input else {
                continue;
            };
            let Some((intermediate, _)) = name.split_once('.') else {
                continue;
            };
            if named.reference(name).is_none() || named.kept(intermediate) {
                continue;
            }
            let text = |value: &str| V::String(value.into());
            findings.push(PreflightFinding::Scalar(Diagnostic {
                code: C::DonorFunctionIntermediate,
                spec_paths: vec![format!("{}.args.{}", located.path, argument.name)],
                context: [
                    ("function".into(), text(&located.call.name)),
                    ("argument".into(), text(&argument.name)),
                    ("intermediate".into(), text(intermediate)),
                    (
                        "reason".into(),
                        text("a donor function argument reads only keep-declared named intermediates"),
                    ),
                ]
                .into(),
                source_span: None,
                operand_route: None,
            }));
        }
    }
    Ok(findings)
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
                crate::project_call_binding::Kind::Joined(cause) => match cause {
                    reference_binding::JoinedFinding::UnknownField => {
                        BindFinding::UnknownReference {
                            path,
                            name: name.clone(),
                        }
                    }
                    reference_binding::JoinedFinding::CurrentKey {
                        identifier,
                        finding,
                    } => BindFinding::QualifiedReference {
                        path,
                        name: identifier,
                        row: row.map(String::from),
                        finding,
                    },
                    reference_binding::JoinedFinding::UnavailableKey { .. } => {
                        BindFinding::ProjectFunction(FunctionFinding {
                            path,
                            function: call.call.name.clone(),
                            argument: Some(argument.name.clone()),
                            node: argument.node,
                            cause: Cause::RowPhase {
                                identifier: name.clone(),
                                row: row.ok_or(BindError::Internal)?.into(),
                            },
                        })
                    }
                    reference_binding::JoinedFinding::NoApplicableKeys { dataset, keys } => {
                        let keys = keys.iter().map(String::as_str).collect::<Vec<_>>();
                        BindFinding::Source(source_expressions::SourceFinding::from_diagnostic(
                            crate::diagnostic::lookup::no_applicable_keys(
                                &path, &dataset, &keys, false,
                            ),
                        ))
                    }
                    reference_binding::JoinedFinding::KeyType {
                        source,
                        expected,
                        actual,
                    } => BindFinding::Source(source_expressions::SourceFinding::from_diagnostic(
                        crate::diagnostic::lookup::key_type(
                            &path,
                            crate::diagnostic::lookup::Owner::Unnamed,
                            &source,
                            expected.into(),
                            actual.into(),
                        ),
                    )),
                },
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
                        row: row.map(String::from),
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
