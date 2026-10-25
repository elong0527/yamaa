//! Bind admitted project calls against immutable types and scope, never cells.
use crate::{
    bound_expression::Read,
    dataset::{BoundProjectFunction, FunctionArgument, FunctionInput},
    project_call_document::Input,
    project_calls::ProjectCalls,
    reference_binding::{self, Binding, Catalog, Diagnostic},
    reference_scope::{self, Scope},
    value::{ColumnType, ValueType},
};

#[derive(Clone, Copy, Debug)]
pub enum SourceMode {
    Record,
    Collect,
}
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    pub source_dataset: usize,
    pub source_mode: SourceMode,
    pub available_outputs: Option<&'a [usize]>,
    pub scope: Scope<'a>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Joined(reference_binding::JoinedFinding),
    Output(Diagnostic),
    Scope(reference_scope::Finding),
    UnsupportedRelation {
        dataset: usize,
    },
    ArgumentType {
        expected: ValueType,
        actual: ValueType,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub argument: usize,
    pub kind: Kind,
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("project call binding invalid call index")]
    InvalidCallIndex,
    #[error("project call binding inactive call")]
    InactiveCall,
    #[error("project call binding reference: {0:?}")]
    Reference(reference_binding::Error),
    #[error("project call binding findings: {0:?}")]
    Findings(Vec<Finding>),
    #[error("project call binding invalid admitted call")]
    InvalidAdmittedCall,
}
#[derive(Clone, Debug, PartialEq)]
pub struct BoundCall {
    pub function: BoundProjectFunction,
    pub dependencies: Vec<usize>,
}
fn kind(column: ColumnType) -> ValueType {
    match column {
        ColumnType::Str => ValueType::Str,
        ColumnType::Int => ValueType::Int,
        ColumnType::Float => ValueType::Float,
        ColumnType::Date => ValueType::Date,
        ColumnType::DateTime => ValueType::DateTime,
    }
}
/// Context is compiler-selected from the row/column phase, never host cells.
/// Invalid arguments retain all independent findings and expose no partial call.
pub fn bind(
    calls: &ProjectCalls,
    call: usize,
    catalog: &Catalog,
    context: Context<'_>,
) -> Result<BoundCall, Error> {
    let prepared = calls.calls().get(call).ok_or(Error::InvalidCallIndex)?;
    let slot = prepared.slot().ok_or(Error::InactiveCall)?;
    let plan = &calls.plans()[slot];
    let mut findings = Vec::new();
    let mut arguments = Vec::new();
    let mut dependencies = Vec::new();
    for (argument, item) in prepared.located.call.arguments.iter().enumerate() {
        let input = match &item.input {
            Input::Literal(value) => Some(FunctionInput::Literal(value.clone())),
            Input::Reference(name) => {
                if let Some(read) = catalog.selected_read(name) {
                    match read {
                        Err(finding) => findings.push(Finding {
                            argument,
                            kind: Kind::Output(finding),
                        }),
                        Ok((read, actual, edges)) => {
                            dependencies.extend(edges);
                            let parameter = plan
                                .signature()
                                .parameters()
                                .iter()
                                .find(|p| p.name == item.name)
                                .ok_or(Error::InvalidAdmittedCall)?;
                            if let Some(actual) =
                                actual.filter(|actual| kind(*actual) != parameter.kind)
                            {
                                findings.push(Finding {
                                    argument,
                                    kind: Kind::ArgumentType {
                                        expected: parameter.kind,
                                        actual: kind(actual),
                                    },
                                });
                            }
                            arguments.push(FunctionArgument {
                                name: item.name.clone(),
                                input: FunctionInput::Read(read),
                            });
                        }
                    }
                    continue;
                }
                if let Some(read) = catalog
                    .joined_read(name, context.available_outputs)
                    .map_err(Error::Reference)?
                {
                    match read {
                        Err(kind) => findings.push(Finding {
                            argument,
                            kind: Kind::Joined(kind),
                        }),
                        Ok((read, actual, edges)) => {
                            dependencies.extend(edges);
                            let parameter = plan
                                .signature()
                                .parameters()
                                .iter()
                                .find(|p| p.name == item.name)
                                .ok_or(Error::InvalidAdmittedCall)?;
                            // A field whose type the schema source does not
                            // decide is not compared.
                            if let Some(actual) =
                                actual.map(kind).filter(|actual| *actual != parameter.kind)
                            {
                                findings.push(Finding {
                                    argument,
                                    kind: Kind::ArgumentType {
                                        expected: parameter.kind,
                                        actual,
                                    },
                                });
                            }
                            arguments.push(FunctionArgument {
                                name: item.name.clone(),
                                input: FunctionInput::Read(read),
                            });
                        }
                    }
                    continue;
                }
                let start = findings.len();
                if name.contains('.') {
                    findings.extend(
                        reference_scope::validate(catalog, name, None, context.scope)
                            .map_err(Error::Reference)?
                            .into_iter()
                            .map(|finding| Finding {
                                argument,
                                kind: Kind::Scope(finding),
                            }),
                    );
                } else if let Some(finding) = catalog
                    .validate_output(
                        name,
                        None,
                        context.available_outputs,
                        &[context.source_dataset],
                    )
                    .map_err(Error::Reference)?
                {
                    findings.push(Finding {
                        argument,
                        kind: Kind::Output(finding),
                    });
                }
                let binding = catalog.bind(name);
                // Driver/existence/output-phase failures retain their established
                // precedence; grouping failures still admit a separate type fault.
                let terminal = findings[start..].iter().any(|f| {
                    matches!(
                        f.kind,
                        Kind::Output(_)
                            | Kind::Scope(
                                reference_scope::Finding::DriverMismatch
                                    | reference_scope::Finding::UnknownField
                            )
                    )
                });
                match binding {
                    Some(binding) if !terminal => {
                        let actual = catalog.type_of(name).map(kind);
                        let input = match binding {
                            Binding::Output { column, .. } => {
                                dependencies.push(column);
                                Some(FunctionInput::Read(Read::Column(column)))
                            }
                            Binding::Dataset { dataset, field, .. } => {
                                if dataset != context.source_dataset {
                                    findings.push(Finding {
                                        argument,
                                        kind: Kind::UnsupportedRelation { dataset },
                                    });
                                    None
                                } else {
                                    Some(match context.source_mode {
                                        SourceMode::Record => {
                                            FunctionInput::Read(Read::Source(field))
                                        }
                                        SourceMode::Collect => FunctionInput::Collect {
                                            column: field,
                                            identifier: name.clone(),
                                        },
                                    })
                                }
                            }
                        };
                        let parameter = plan
                            .signature()
                            .parameters()
                            .iter()
                            .find(|p| p.name == item.name)
                            .ok_or(Error::InvalidAdmittedCall)?;
                        if let Some(actual) = actual.filter(|actual| *actual != parameter.kind) {
                            findings.push(Finding {
                                argument,
                                kind: Kind::ArgumentType {
                                    expected: parameter.kind,
                                    actual,
                                },
                            });
                        }
                        input
                    }
                    _ => None,
                }
            }
        };
        if let Some(input) = input {
            arguments.push(FunctionArgument {
                name: item.name.clone(),
                input,
            });
        }
    }
    if !findings.is_empty() {
        return Err(Error::Findings(findings));
    }
    let function = BoundProjectFunction::new_project(slot, plan.clone(), arguments)
        .map_err(|_| Error::InvalidAdmittedCall)?;
    Ok(BoundCall {
        function,
        dependencies,
    })
}
