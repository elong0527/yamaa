//! Bind admitted catalogue metadata without code, source access or dictionary resolution.
use crate::{
    dataset::{Check, Verification},
    diagnostic::{ConditionCode as C, Diagnostic},
    project_terminology::{BindingFinding, Catalogue},
    value::{ColumnType, Value},
};
use serde_json::Value as V;
use std::vec;

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
    pub fn cause(&self) -> &Cause {
        &self.cause
    }
    pub fn diagnostic(&self) -> Diagnostic {
        let mut paths = vec![self.path.clone()];
        let mut context: crate::diagnostic::Context = [
            ("column".into(), V::String(self.column.clone())),
            ("codelist".into(), V::String(self.codelist.clone())),
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
                context.insert("codelist_data_type".into(), V::String(datatype.into()));
                context.insert("column_type".into(), V::String(kind(*actual).into()));
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
                    V::Array(codelist_values.iter().cloned().map(V::from).collect()),
                );
                context.insert(
                    "allowed_values".into(),
                    V::Array(allowed_values.iter().cloned().map(V::from).collect()),
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
pub struct BoundCodelist {
    pub column: usize,
    pub verification: Verification,
}
/// Bind every admitted codelist and retain all independent semantic findings.
/// Extensible/external bindings retain their ordinary explicit verification only.
pub fn bind(
    catalogue: &Catalogue,
    bindings: &[Binding<'_>],
) -> Result<Vec<BoundCodelist>, Vec<Finding>> {
    if bindings.is_empty() {
        return Ok(Vec::new());
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
                BindingFinding::ValuesConflict => unreachable!("no explicit value set supplied"),
            }));
        }
        for allowed in binding.allowed_values {
            for fault in catalogue.bind(binding.codelist, binding.kind, Some(allowed.values)) {
                // Unknown/type findings were emitted once per binding above.
                if let BindingFinding::ValuesConflict = fault {
                    findings.push(make(Cause::ValuesConflict {
                        verification: allowed.path.into(),
                        codelist_values: catalogue
                            .enforced_items(binding.codelist)
                            .expect("only fixed item lists conflict")
                            .iter()
                            .map(|item| item.value.clone())
                            .collect(),
                        allowed_values: allowed.values.to_vec(),
                    }));
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
        Err(findings)
    }
}
