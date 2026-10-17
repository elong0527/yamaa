//! Bind admitted catalogue metadata without code, source access or dictionary resolution.
use crate::json::Value as V;
use crate::{
    dataset::{Check, Verification},
    diagnostic::{ConditionCode as C, Diagnostic},
    project_terminology::{BindingFinding, Catalogue},
    value::ColumnType,
};
use std::vec;

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
    /// Paths of the column's `allowed_values` verifications. Only a column-level
    /// binding is compared with them; a value-level binding supplies none.
    pub allowed_values: &'a [&'a str],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cause {
    Unknown,
    TypeMismatch {
        expected: ColumnType,
        actual: ColumnType,
    },
    /// The `allowed_values` verification at this path restates the bound items.
    Redundant {
        verification: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    path: String,
    column: String,
    codelist: String,
    cause: Cause,
}
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
            Cause::Redundant { verification } => {
                // The redundant declaration is the verification; the binding
                // that already states the value set is reported beside it.
                paths = vec![verification.clone()];
                context.insert("stated_by".into(), V::String(self.path.clone()));
                C::CodelistAllowedValuesRedundant
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
/// Every list of items is enforced; an external dictionary binding is not.
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
        let faults = catalogue.bind(
            binding.codelist,
            binding.kind,
            !binding.allowed_values.is_empty(),
        );
        for fault in faults {
            match fault {
                BindingFinding::UnknownCodelist => findings.push(make(Cause::Unknown)),
                BindingFinding::TypeMismatch { expected, actual } => {
                    findings.push(make(Cause::TypeMismatch { expected, actual }))
                }
                BindingFinding::RedundantAllowedValues => {
                    for verification in binding.allowed_values {
                        findings.push(make(Cause::Redundant {
                            verification: (*verification).into(),
                        }));
                    }
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
