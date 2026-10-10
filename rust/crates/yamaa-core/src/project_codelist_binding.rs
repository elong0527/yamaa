//! Bind admitted catalogue metadata without code, source access or dictionary resolution.
use crate::{
    dataset::{Check, Verification},
    diagnostic::{ConditionCode as C, ContextValue as V, Diagnostic},
    project_limits::{Budget, Limit, Limits, Resource},
    project_terminology::{BindingFinding, Catalogue},
    value::{ColumnType, Value},
};
use alloc::{string::String, vec, vec::Vec};

#[derive(Clone, Copy, Debug)]
pub struct AllowedValues<'a> {
    pub path: &'a str,
    pub values: &'a [Value],
}
#[derive(Clone, Copy, Debug)]
pub enum Path<'a> {
    Authored(&'a str),
    Column(&'a str),
}
impl Path<'_> {
    fn parts(&self) -> [&str; 3] {
        match self {
            Self::Authored(path) => ["", path, ""],
            Self::Column(name) => ["columns.", name, ".submission.codelist"],
        }
    }
    fn owned(&self) -> String {
        self.parts().concat()
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Binding<'a> {
    pub column: usize,
    pub name: &'a str,
    pub kind: ColumnType,
    pub path: Path<'a>,
    pub codelist: &'a str,
    pub allowed_values: &'a [AllowedValues<'a>],
}
#[derive(Clone, Debug, PartialEq)]
pub enum Cause {
    Unknown,
    TypeMismatch {
        expected: ColumnType,
        actual: ColumnType,
    },
    ValuesConflict {
        verification: String,
        codelist_values: Vec<Value>,
        allowed_values: Vec<Value>,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    path: String,
    column: String,
    codelist: String,
    cause: Cause,
}
// All Value floats are finite by construction; structural equality is reflexive.
impl Eq for Finding {}
impl Finding {
    /// Borrow variable-size diagnostic inputs before a formatter allocates them.
    pub fn visit_diagnostic_text(&self, mut visit: impl FnMut(&str)) {
        for text in [&self.path, &self.column, &self.codelist] {
            visit(text);
        }
        if let Cause::ValuesConflict {
            verification,
            codelist_values,
            allowed_values,
        } = &self.cause
        {
            visit(verification);
            for value in codelist_values.iter().chain(allowed_values) {
                visit(match value {
                    Value::Str(text) => text,
                    // Every other admitted scalar has bounded canonical text.
                    _ => "0123456789012345678901234567890123456789012345678901234567890123",
                });
            }
        }
    }
    pub fn cause(&self) -> &Cause {
        &self.cause
    }
    pub fn diagnostic(&self) -> Diagnostic {
        let mut paths = vec![self.path.clone()];
        let mut context: crate::diagnostic::Context = [
            ("column".into(), V::Scalar(Value::Str(self.column.clone()))),
            (
                "codelist".into(),
                V::Scalar(Value::Str(self.codelist.clone())),
            ),
        ]
        .into();
        let code = match &self.cause {
            Cause::Unknown => C::CodelistUnknownBinding,
            Cause::TypeMismatch { expected, actual } => {
                fn kind(kind: ColumnType) -> &'static str {
                    match kind {
                        ColumnType::Str => "str",
                        ColumnType::Int => "int",
                        ColumnType::Float => "float",
                        ColumnType::Date => "date",
                        ColumnType::DateTime => "datetime",
                    }
                }
                let datatype = match expected {
                    ColumnType::Str => "text",
                    ColumnType::Int => "integer",
                    ColumnType::Float => "float",
                    _ => unreachable!("catalogue admits only text and numeric types"),
                };
                context.insert(
                    "codelist_data_type".into(),
                    V::Scalar(Value::Str(datatype.into())),
                );
                context.insert(
                    "column_type".into(),
                    V::Scalar(Value::Str(kind(*actual).into())),
                );
                C::CodelistBindingTypeMismatch
            }
            Cause::ValuesConflict {
                verification,
                codelist_values,
                allowed_values,
            } => {
                paths.push(verification.clone());
                context.insert(
                    "codelist_values".into(),
                    V::Sequence(codelist_values.iter().cloned().map(V::Scalar).collect()),
                );
                context.insert(
                    "allowed_values".into(),
                    V::Sequence(allowed_values.iter().cloned().map(V::Scalar).collect()),
                );
                C::CodelistBindingValuesConflict
            }
        };
        Diagnostic {
            code,
            spec_paths: paths,
            context,
            source_span: None,
            operand_route: None,
        }
    }
}
#[derive(Debug)]
pub enum Error {
    Limit(Limit),
    Findings(Vec<Finding>),
}
impl From<Limit> for Error {
    fn from(limit: Limit) -> Self {
        Self::Limit(limit)
    }
}
#[derive(Debug)]
pub struct BoundCodelist {
    pub column: usize,
    pub verification: Verification,
}
fn product(left: usize, right: usize) -> Result<usize, Error> {
    left.checked_mul(right).ok_or(Error::Limit(Limit {
        resource: Resource::Work,
    }))
}
/// Admit the complete aggregate before the first owned finding or code copy.
/// Extensible/external bindings retain their ordinary explicit verification only.
pub fn bind(
    catalogue: &Catalogue,
    bindings: &[Binding<'_>],
    limits: Limits,
) -> Result<Vec<BoundCodelist>, Error> {
    if bindings.is_empty() {
        return Ok(Vec::new());
    }
    let mut budget = Budget::new(limits);
    let lists = catalogue
        .sources()
        .iter()
        .flat_map(|source| &source.codelists);
    let mut count = 0usize;
    let mut maximum_id = 1usize;
    for list in lists {
        budget.work(1)?;
        count = count.checked_add(1).ok_or(Error::Limit(Limit {
            resource: Resource::Work,
        }))?;
        maximum_id = maximum_id.max(list.id.len());
    }
    for binding in bindings {
        let searches = binding
            .allowed_values
            .len()
            .checked_mul(3)
            .and_then(|n| n.checked_add(4))
            .ok_or(Error::Limit(Limit {
                resource: Resource::Work,
            }))?;
        budget.work(product(
            product(count, searches)?,
            maximum_id.max(binding.codelist.len()).max(1),
        )?)?;
        let finding_count = binding
            .allowed_values
            .len()
            .checked_add(2)
            .ok_or(Error::Limit(Limit {
                resource: Resource::Findings,
            }))?;
        budget.findings(finding_count)?;
        // Findings, diagnostic context and future plan ownership each copy these.
        for _ in 0..finding_count.checked_mul(3).ok_or(Error::Limit(Limit {
            resource: Resource::TextBytes,
        }))? {
            for text in [binding.name, binding.codelist]
                .into_iter()
                .chain(binding.path.parts())
            {
                budget.text(text)?;
            }
        }
        if let Some(items) = catalogue.enforced_items(binding.codelist) {
            // One compiler check and its future bound plan; semantic comparison
            // and all potential conflict payloads are charged independently.
            budget.binding(items, &[])?;
            budget.binding(items, &[])?;
            for allowed in binding.allowed_values {
                for _ in 0..3 {
                    budget.binding(items, allowed.values)?;
                    budget.text(allowed.path)?;
                }
            }
        }
    }
    let mut findings = Vec::new();
    let mut checks = Vec::new();
    for binding in bindings {
        let make = |cause| Finding {
            path: binding.path.owned(),
            column: binding.name.into(),
            codelist: binding.codelist.into(),
            cause,
        };
        let faults = catalogue.bind(binding.codelist, binding.kind, None);
        for fault in faults {
            findings.push(make(match fault {
                BindingFinding::UnknownCodelist => Cause::Unknown,
                BindingFinding::TypeMismatch { expected, actual } => {
                    Cause::TypeMismatch { expected, actual }
                }
                BindingFinding::Limit(limit) => return Err(Error::Limit(limit)),
                BindingFinding::ValuesConflict => unreachable!("no explicit value set supplied"),
            }));
        }
        for allowed in binding.allowed_values {
            for fault in catalogue.bind(binding.codelist, binding.kind, Some(allowed.values)) {
                match fault {
                    BindingFinding::ValuesConflict => findings.push(make(Cause::ValuesConflict {
                        verification: allowed.path.into(),
                        codelist_values: catalogue
                            .enforced_items(binding.codelist)
                            .expect("only fixed item lists conflict")
                            .iter()
                            .map(|item| item.value.clone())
                            .collect(),
                        allowed_values: allowed.values.to_vec(),
                    })),
                    BindingFinding::Limit(limit) => return Err(Error::Limit(limit)),
                    _ => {} // Unknown/type findings already emitted once per binding.
                }
            }
        }
        if let Some(items) = catalogue.enforced_items(binding.codelist) {
            checks.push(BoundCodelist {
                column: binding.column,
                verification: Verification {
                    path: binding.path.owned(),
                    check: Check::Codelist {
                        id: binding.codelist.into(),
                        values: items.iter().map(|item| item.value.clone()).collect(),
                    },
                },
            });
        }
    }
    if findings.is_empty() {
        Ok(checks)
    } else {
        Err(Error::Findings(findings))
    }
}
