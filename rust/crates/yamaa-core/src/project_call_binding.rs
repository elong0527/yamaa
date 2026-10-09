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
use alloc::vec::Vec;

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
#[derive(Debug)]
pub enum Error {
    InvalidCallIndex,
    InactiveCall,
    Reference(reference_binding::Error),
    Findings(Vec<Finding>),
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
                let start = findings.len();
                if name.contains('.') {
                    findings.extend(
                        reference_scope::validate(
                            catalog,
                            name,
                            None,
                            context.scope,
                            Default::default(),
                        )
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
                let binding = catalog.bind(name).map_err(Error::Reference)?;
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
                        let (input, actual) = match binding {
                            Binding::Output {
                                column,
                                column_type,
                            } => {
                                dependencies.push(column);
                                (
                                    Some(FunctionInput::Read(Read::Column(column))),
                                    kind(column_type),
                                )
                            }
                            Binding::Dataset {
                                dataset,
                                field,
                                column_type,
                            } => {
                                let input = if dataset != context.source_dataset {
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
                                };
                                (input, kind(column_type))
                            }
                        };
                        let parameter = plan
                            .signature()
                            .parameters()
                            .iter()
                            .find(|p| p.name == item.name)
                            .ok_or(Error::InvalidAdmittedCall)?;
                        if actual != parameter.kind {
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
