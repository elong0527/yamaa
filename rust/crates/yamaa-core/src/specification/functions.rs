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
    /// The called function's file names no callable for the host language
    /// (FUNC-1).
    MissingImplementation(crate::project_function::Language),
    /// A 1.x specification checked or run without a project file: no project
    /// supplies the function, so the call fails as `project_binding_missing`
    /// (CT-13).
    NoProject,
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
    /// A call or an argument of a shape no normalized call has, or a
    /// written value the engine cannot run with: the decoder's kind and
    /// the text an out-of-range integer, an invalid date or an invalid
    /// datetime is written as.
    Scalar {
        kind: crate::project_function_document::Kind,
        value: Option<String>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionFinding {
    pub path: String,
    pub function: String,
    pub argument: Option<String>,
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
            Cause::MissingImplementation(language) => {
                context.insert("language".into(), text(language.key()));
                (C::FunctionImplementationMissing, None)
            }
            Cause::NoProject => {
                context.insert(
                    "binding".into(),
                    text(crate::expression_table::Verb::Function.name()),
                );
                (C::ProjectBindingMissing, None)
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
            Cause::Scalar { kind, ref value } => {
                // A fault of one argument names the argument and the text it
                // is written as, null when it is no scalar text (D253).
                if self.argument.is_some() {
                    context.insert("value".into(), value.as_deref().map_or(V::Null, text));
                }
                (
                    C::InvalidFunctionArgument,
                    Some(match kind {
                        crate::project_function_document::Kind::NormalizedShape => "invalid_shape",
                        crate::project_function_document::Kind::IntegerRange => "integer_range",
                        crate::project_function_document::Kind::InvalidDate => "invalid_date",
                        crate::project_function_document::Kind::InvalidDateTime => {
                            "invalid_datetime"
                        }
                    }),
                )
            }
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
    /// Each admitted call's index, by its call expression.
    pub indices: BTreeMap<ExprId, usize>,
    /// The return type each call's definition declares, by its expression.
    pub returns: BTreeMap<ExprId, ColumnType>,
    /// The calls each derivation evaluates, by the derivation's root.
    sites: BTreeMap<ExprId, Vec<usize>>,
    /// The calls intermediate derivations evaluate.
    bindings: Vec<usize>,
}
impl Prepared {
    /// The admitted call of the call expression `call`.
    pub(super) fn call(&self, call: ExprId) -> Option<usize> {
        self.indices.get(&call).copied()
    }
    /// The calls of the derivation whose root is `root`.
    pub(super) fn site_calls(&self, root: ExprId) -> &[usize] {
        self.sites.get(&root).map_or(&[], Vec::as_slice)
    }
    /// Each call's uses in a spec with row templates: one for each row
    /// declaration (by its root) that evaluates it, and one for each
    /// intermediate derivation.
    pub(super) fn row_uses(
        &self,
        declarations: impl Iterator<Item = ExprId>,
    ) -> Result<Vec<usize>, PrepareError> {
        let mut uses = std::vec![0usize; self.calls.calls().len()];
        for &call in declarations
            .flat_map(|root| self.site_calls(root))
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

/// One call a derivation evaluates: its expression, its path, the path of
/// that derivation and whether its references read a record context (an
/// aggregate binding or a donor).
struct Request {
    call: ExprId,
    path: String,
    derivation: String,
    /// The call's position in authored order ([`LocatedCall::authored`]).
    authored: usize,
    contextual: bool,
}

/// The spec path of the derivation a call site evaluates: a column's, an
/// intermediate's or a row template's.
fn derivation_path(spec: &ast::Spec, site: &ast::Site) -> String {
    use crate::ast::Owner;
    let derivation = match site.owner {
        Owner::Default { column } => spec.columns[column].derivation.as_ref(),
        Owner::Binding {
            intermediate,
            binding,
        } => Some(&spec.intermediates[intermediate].bindings[binding].derivation),
        Owner::Override { template, entry } => {
            Some(&spec.rows.templates[template].overrides[entry].1)
        }
        _ => None,
    };
    derivation.map_or_else(
        || checks::path(spec, site.root),
        |derivation| checks::render(spec, derivation.meta),
    )
}

/// Every call the derivations evaluate, in today's occurrence order:
/// columns, then intermediate derivations, then row template overrides.
fn requests(spec: &ast::Spec) -> (Vec<Request>, BTreeMap<ExprId, Vec<usize>>, Vec<usize>) {
    use crate::ast::{Owner, Scope, Step};
    let sites = spec.sites();
    let mut requests = Vec::new();
    let mut by_site = BTreeMap::new();
    let mut bindings = Vec::new();
    let passes: [fn(&Owner) -> bool; 3] = [
        |owner| matches!(owner, Owner::Default { .. }),
        |owner| matches!(owner, Owner::Binding { .. }),
        |owner| matches!(owner, Owner::Override { .. }),
    ];
    let mut counts = [0usize; 3];
    for (pass, owns) in passes.into_iter().enumerate() {
        for site in sites.iter().filter(|site| owns(&site.owner)) {
            let derivation = derivation_path(spec, site);
            let mut indices = Vec::new();
            spec.walk(site.root, site.scope, |at, scope| {
                if matches!(spec.expr(at).kind, ExprKind::Call { .. }) {
                    indices.push(requests.len());
                    counts[pass] += 1;
                    requests.push(Request {
                        call: at,
                        path: checks::path(spec, at),
                        derivation: derivation.clone(),
                        authored: 0,
                        contextual: scope != Scope::Row,
                    });
                }
                Step::Continue
            });
            if matches!(site.owner, Owner::Binding { .. }) {
                bindings.extend(&indices);
            }
            by_site.insert(site.root, indices);
        }
    }
    // A formatted file writes the intermediates before the columns and the
    // row templates after them (yaml/schema_spec.yaml): the intermediates'
    // calls come first in authored order, each pass keeping its own order
    // (D221).
    let [columns, intermediates, _] = counts;
    for (index, request) in requests.iter_mut().enumerate() {
        request.authored = match index {
            index if index < columns => intermediates + index,
            index if index < columns + intermediates => index - columns,
            index => index,
        };
    }
    (requests, by_site, bindings)
}

/// A call's decoding fault, and the argument it is a fault of: none for the
/// shape of the call itself.
type DecodeFault = (crate::project_function_document::Finding, Option<String>);

/// A call as written: its function's name and each argument, a name read
/// or a written value. Every value the engine cannot run with is refused
/// at its argument, and no partial call is admitted.
fn decode(
    spec: &ast::Spec,
    call: ExprId,
    path: &str,
) -> Result<project_call_document::Call, Vec<DecodeFault>> {
    use crate::project_function_document::{Finding, Kind};
    use crate::value::Value;
    let shape_fault = |path: String| {
        let fault = Finding {
            path,
            node: call.index(),
            kind: Kind::NormalizedShape,
            value: None,
        };
        (fault, None)
    };
    let ExprKind::Call {
        function,
        args,
        shape,
    } = &spec.expr(call).kind
    else {
        return Err(vec![shape_fault(path.into())]);
    };
    let mut findings = Vec::new();
    // Only `name` and `args`, a mapping, as a normalized call writes them.
    if !shape.closed {
        findings.push(shape_fault(path.into()));
    }
    if !shape.args {
        findings.push(shape_fault(format!("{path}.args")));
    }
    let mut arguments = Vec::new();
    for (name, value) in args {
        let path = format!("{path}.args.{}", name.name);
        let mut refuse = |kind, value: Option<&String>| {
            let fault = Finding {
                path: path.clone(),
                node: call.index(),
                kind,
                value: value.cloned(),
            };
            findings.push((fault, Some(name.name.clone())));
            Value::Missing
        };
        let input = match &spec.expr(*value).kind {
            ExprKind::Ref(reference) => Input::Reference(reference.0.clone()),
            ExprKind::Literal(literal) => Input::Literal(match literal {
                ast::Literal::Missing => Value::Missing,
                ast::Literal::Bool(value) => Value::Bool(*value),
                ast::Literal::Text(value) => Value::Str(value.clone()),
                ast::Literal::Float(value) => Value::float(*value),
                ast::Literal::Int(text) => match text.parse::<i64>() {
                    Ok(value) => Value::Int(value),
                    Err(_) => refuse(Kind::IntegerRange, Some(text)),
                },
                ast::Literal::Date(text) => match text.parse() {
                    Ok(value) => Value::Date(value),
                    Err(_) => refuse(Kind::InvalidDate, Some(text)),
                },
                ast::Literal::DateTime(text) => match text.parse() {
                    Ok(value) => Value::DateTime(value),
                    Err(_) => refuse(Kind::InvalidDateTime, Some(text)),
                },
                ast::Literal::Unsupported => refuse(Kind::NormalizedShape, None),
            }),
            _ => Input::Literal(refuse(Kind::NormalizedShape, None)),
        };
        arguments.push(project_call_document::Argument {
            name: name.name.clone(),
            node: value.index(),
            input,
        });
    }
    if findings.is_empty() {
        Ok(project_call_document::Call {
            name: function.name.clone(),
            name_node: call.index(),
            node: call.index(),
            arguments,
        })
    } else {
        Err(findings)
    }
}

pub(super) fn prepare(
    spec: &ast::Spec,
    functions: &[Function],
    projectless: bool,
) -> Result<Prepared, PrepareError> {
    // A specification compiled without a project file reports the missing
    // project as the cause of each call to a function no project supplies
    // (CT-13); with a project, the unknown name is the cause.
    let unknown = || {
        if projectless {
            Cause::NoProject
        } else {
            Cause::UnknownFunction
        }
    };
    let mut function_index = BTreeMap::new();
    for function in functions {
        if function_index.insert(function.name(), function).is_some() {
            return Err(PrepareError::Internal);
        }
    }
    let (requests, sites, bindings) = requests(spec);
    let has_rows = !spec.rows.templates.is_empty();
    let mut calls = Vec::new();
    let mut origins = Vec::new();
    let mut findings = Vec::new();
    for (request, Request { call, path, .. }) in requests.iter().enumerate() {
        match decode(spec, *call, path) {
            Ok(decoded) => {
                // A 1.0 function file may name no callable for the host: the
                // call fails here, before any data is read, and no other
                // finding of it is reported (FUNC-1). An engine that claims no
                // host language misses no callable; it compiles the call, and
                // a run of it is refused instead (D110).
                let missing = function_index
                    .get(decoded.name.as_str())
                    .and_then(|function| function.missing_implementation());
                if let Some(language) = missing {
                    findings.push((
                        request,
                        invalid(FunctionFinding {
                            path: format!("{path}.name"),
                            function: decoded.name,
                            argument: None,
                            cause: Cause::MissingImplementation(language),
                        }),
                    ));
                } else {
                    origins.push((request, *call, path.clone(), decoded.name.clone()));
                    calls.push(LocatedCall {
                        path: path.clone(),
                        derivation: requests[request].derivation.clone(),
                        authored: requests[request].authored,
                        call: decoded,
                    });
                }
            }
            Err(errors) => {
                let function = match &spec.expr(*call).kind {
                    ExprKind::Call { function, .. } => function.name.as_str(),
                    _ => "",
                };
                if !function.is_empty() && !function_index.contains_key(function) {
                    findings.push((
                        request,
                        invalid(FunctionFinding {
                            path: format!("{path}.name"),
                            function: function.into(),
                            argument: None,
                            cause: unknown(),
                        }),
                    ));
                }
                findings.extend(errors.into_iter().map(|(error, argument)| {
                    (
                        request,
                        invalid(FunctionFinding {
                            path: error.path,
                            function: function.into(),
                            argument,
                            cause: Cause::Scalar {
                                kind: error.kind,
                                value: error.value,
                            },
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
                        match &a.input {
                            Input::Reference(name) => Some(name.clone()),
                            _ => None,
                        },
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let types = spec
        .columns
        .iter()
        .map(|column| (column.name.name.as_str(), ValueType::from(column.kind)))
        .collect::<BTreeMap<_, _>>();
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
                    let (origin, _, path, function) = &origins[error.call];
                    let arguments = &argument_origins[error.call];
                    let (cause, argument, path) = match error.kind {
                        project_calls::Kind::UnknownFunction => {
                            (unknown(), None, format!("{path}.name"))
                        }
                        project_calls::Kind::UnknownReference { argument } => {
                            let (name, reference) = &arguments[argument];
                            (
                                Cause::UnknownReference(
                                    reference.clone().ok_or(PrepareError::Internal)?,
                                ),
                                Some(name.clone()),
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
                                            cause: Cause::MissingRequiredArgument,
                                        }),
                                    ));
                                    continue;
                                }
                            };
                            let (argument, _) = &arguments[index.ok_or(PrepareError::Internal)?];
                            (
                                cause,
                                Some(argument.clone()),
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
        .map(|(index, (_, call, _, _))| {
            Ok((
                *call,
                definitions[index]
                    .ok_or(PrepareError::Internal)?
                    .definition()
                    .returns,
            ))
        })
        .collect::<Result<_, PrepareError>>()?;
    let indices = origins
        .into_iter()
        .enumerate()
        .map(|(index, (_, call, _, _))| (call, index))
        .collect();
    Ok(Prepared {
        calls,
        indices,
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
    root: ExprId,
    named: &intermediates::Declarations,
) -> Result<Vec<PreflightFinding>, PrepareError> {
    let mut findings = Vec::new();
    for &call in project.site_calls(root) {
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
                        text("a donor function argument reads only named intermediates that declare a selection (keep or nearest)"),
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
            if let crate::project_call_binding::Kind::Joined(cause) = &error.kind {
                if let Some(diagnostic) = cause.key_diagnostic(&path) {
                    return Ok(BindFinding::Source(
                        source_expressions::SourceFinding::from_diagnostic(diagnostic),
                    ));
                }
            }
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
                            cause: Cause::RowPhase {
                                identifier: name.clone(),
                                row: row.ok_or(BindError::Internal)?.into(),
                            },
                        })
                    }
                    reference_binding::JoinedFinding::UndeclaredKey { .. }
                    | reference_binding::JoinedFinding::DonorKey { .. }
                    | reference_binding::JoinedFinding::MatchValue { .. }
                    | reference_binding::JoinedFinding::KeyType { .. } => {
                        return Err(BindError::Internal)
                    }
                },
                crate::project_call_binding::Kind::Output(
                    crate::reference_binding::Diagnostic::PhaseBoundary { .. },
                ) => BindFinding::ProjectFunction(FunctionFinding {
                    path,
                    function: call.call.name.clone(),
                    argument: Some(argument.name.clone()),
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
