//! Compile original output-window declarations into the existing shared evaluator.
use super::*;
use crate::{
    bound_expression::BoundPredicate,
    dataset::{OrderTerm, Window, WindowKind},
    predicate_compiler,
};

#[derive(Clone, Debug)]
enum Kind {
    RowNumber,
    Competition,
    Dense,
    RowValue {
        source: String,
        offset: i64,
    },
    PreviousNonMissing(String),
    Locf(String),
    BaselineFlag {
        date: String,
        reference_date: String,
    },
}
#[derive(Clone, Debug)]
pub(super) struct Declaration {
    kind: Kind,
    operation: String,
    path: String,
    groups: Vec<String>,
    order: Vec<(String, bool, bool)>,
    filter: Option<(
        String,
        Result<crate::predicate::Plan, predicate_compiler::Error>,
    )>,
}
#[derive(Debug)]
pub enum WindowFinding {
    ZeroOffset {
        path: String,
    },
    Order {
        path: String,
        operation: String,
        forbidden: bool,
    },
}
impl WindowFinding {
    pub fn definition(&self) -> crate::diagnostic::Definition {
        let (condition, requirement) = match self {
            Self::ZeroOffset { .. } => ("zero_offset", "REQ-0328"),
            Self::Order {
                forbidden: false, ..
            } => ("window_order_by_required", "REQ-0340"),
            Self::Order {
                forbidden: true, ..
            } => ("window_order_by_forbidden", "REQ-0341"),
        };
        crate::diagnostic::Definition {
            phase: "validation",
            condition,
            requirement,
        }
    }
}
impl Declaration {
    pub fn prepare(
        d: &Document,
        payload: usize,
        op: &str,
        path: &str,
        limits: CompilationLimits,
        extra: &mut Vec<UnsupportedFeature>,
    ) -> Result<Self, PrepareError> {
        let name = |field_name| text(d, field(d, payload, field_name)?).map(String::from);
        let kind = match op {
            "row_number" => Kind::RowNumber,
            "rank" => match d
                .field(payload, "method")
                .map(|id| text(d, id))
                .transpose()?
                .unwrap_or("competition")
            {
                "competition" => Kind::Competition,
                "dense" => Kind::Dense,
                _ => return Err(PrepareError::Internal),
            },
            "row_value" => {
                let N::Integer(value) = &d.nodes()[field(d, payload, "offset")?] else {
                    return Err(PrepareError::Internal);
                };
                let offset = value.parse().map_err(|_| {
                    PrepareError::Unsupported(vec![UnsupportedFeature {
                        operation: "wide_window_offset".into(),
                        path: format!("{path}.offset"),
                    }])
                })?;
                Kind::RowValue {
                    source: name("source")?,
                    offset,
                }
            }
            "previous_non_missing" => Kind::PreviousNonMissing(name("source")?),
            "locf" => Kind::Locf(name("source")?),
            "baseline_flag" => Kind::BaselineFlag {
                date: name("date")?,
                reference_date: name("reference_date")?,
            },
            _ => return Err(PrepareError::Internal),
        };
        let mut groups = Vec::new();
        let mut order: Vec<(String, bool, bool)> = Vec::new();
        let mut filter = None;
        if let Some(window) = d
            .field(payload, "window")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        {
            // Shared schema admission already expanded named windows, preserving
            // their origin links; this compiler only consumes their admitted data.
            if let Some(group) = d
                .field(window, "group_by")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                let items = sequence(d, group)?;
                if items.len() > limits.keys {
                    return Err(PrepareError::Limit("window_groups"));
                }
                groups = items
                    .iter()
                    .map(|&id| text(d, id).map(String::from))
                    .collect::<Result<_, _>>()?;
            }
            if let Some(terms) = d
                .field(window, "order_by")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                let items = sequence(d, terms)?;
                if items.len() > limits.source_fields {
                    return Err(PrepareError::Limit("window_order"));
                }
                for &term in items {
                    let (variable, descending, nulls_first) =
                        if let N::Text(value) = &d.nodes()[term] {
                            (value.as_str(), false, false)
                        } else {
                            (
                                text(d, field(d, term, "variable")?)?,
                                d.field(term, "direction")
                                    .map(|id| text(d, id))
                                    .transpose()?
                                    .unwrap_or("asc")
                                    == "desc",
                                d.field(term, "nulls")
                                    .map(|id| text(d, id))
                                    .transpose()?
                                    .unwrap_or("last")
                                    == "first",
                            )
                        };
                    order.push((variable.into(), descending, nulls_first));
                }
            }
            if let Some(id) = d
                .field(window, "filter")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                let expression = text(d, id)?;
                let filter_path = format!("{path}.window.filter");
                let compiled =
                    predicate_compiler::compile(expression, &filter_path, Default::default());
                match &compiled {
                    Ok(_)
                    | Err(predicate_compiler::Error::Parse(
                        crate::predicate_parser::ParseError::Grammar { .. },
                    )) => {}
                    Err(predicate_compiler::Error::UnsupportedLiteral { .. }) => {
                        reject(extra, "predicate_literal", filter_path)
                    }
                    Err(predicate_compiler::Error::Parse(
                        crate::predicate_parser::ParseError::UnsupportedRegex { .. },
                    )) => reject(extra, "predicate_regex", filter_path),
                    Err(predicate_compiler::Error::Internal) => return Err(PrepareError::Internal),
                    Err(_) => return Err(PrepareError::Limit("predicate_compilation")),
                }
                filter = Some((expression.into(), compiled));
            }
        }
        let mut names = groups
            .iter()
            .map(String::as_str)
            .chain(order.iter().map(|term| term.0.as_str()))
            .collect::<Vec<_>>();
        match &kind {
            Kind::RowValue { source, .. }
            | Kind::PreviousNonMissing(source)
            | Kind::Locf(source) => names.push(source),
            Kind::BaselineFlag {
                date,
                reference_date,
            } => {
                names.push(date);
                names.push(reference_date);
            }
            _ => {}
        }
        if let Some((_, Ok(plan))) = &filter {
            names.extend(plan.identifiers());
        }
        if names.iter().any(|name| name.contains('.')) {
            reject(extra, "qualified_window_reference", path.into());
        }
        Ok(Self {
            kind,
            operation: op.into(),
            path: path.into(),
            groups,
            order,
            filter,
        })
    }
    pub fn bind(
        &self,
        catalog: &Catalog,
        edges: &mut Vec<usize>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<Option<Window>, BindError> {
        let start = findings.len();
        if matches!(self.kind, Kind::RowValue { offset: 0, .. }) {
            findings.push(BindFinding::Window(WindowFinding::ZeroOffset {
                path: format!("{}.offset", self.path),
            }));
        }
        let baseline = matches!(self.kind, Kind::BaselineFlag { .. });
        if self.order.is_empty() != baseline {
            findings.push(BindFinding::Window(WindowFinding::Order {
                path: format!(
                    "{}.window{}",
                    self.path,
                    if baseline { ".order_by" } else { "" }
                ),
                operation: self.operation.clone(),
                forbidden: baseline,
            }));
        }
        let filter = match &self.filter {
            Some((
                text,
                Err(predicate_compiler::Error::Parse(
                    crate::predicate_parser::ParseError::Grammar {
                        position, failure, ..
                    },
                )),
            )) => {
                findings.push(BindFinding::Lookup(LookupFinding::grammar(
                    &format!("{}.window.filter", self.path),
                    text,
                    position.character,
                    failure,
                )));
                None
            }
            Some((_, Err(error))) => return Err(BindError::PredicatePolicy(error.clone())),
            Some((_, Ok(plan))) => Some(plan),
            None => None,
        };
        let mut resolve = |name: &str, path: String| -> Result<usize, BindError> {
            if let Some(finding) = catalog
                .validate_output(name, None, None, &[0])
                .map_err(BindError::Catalog)?
            {
                findings.push(BindFinding::OutputReference {
                    path,
                    name: name.into(),
                    finding,
                });
                return Ok(0);
            }
            let Some(reference_binding::Binding::Output { column, .. }) =
                catalog.bind(name).map_err(BindError::Catalog)?
            else {
                return Err(BindError::Internal);
            };
            edges.push(column);
            Ok(column)
        };
        let kind = match &self.kind {
            Kind::RowNumber => WindowKind::RowNumber,
            Kind::Competition => WindowKind::Competition,
            Kind::Dense => WindowKind::Dense,
            Kind::RowValue { source, offset } => WindowKind::RowValue {
                column: resolve(source, format!("{}.source", self.path))?,
                offset: *offset,
            },
            Kind::PreviousNonMissing(source) => WindowKind::PreviousNonMissing {
                column: resolve(source, format!("{}.source", self.path))?,
            },
            Kind::Locf(source) => WindowKind::Locf {
                column: resolve(source, format!("{}.source", self.path))?,
            },
            Kind::BaselineFlag {
                date,
                reference_date,
            } => WindowKind::BaselineFlag {
                date: resolve(date, format!("{}.date", self.path))?,
                reference_date: resolve(reference_date, format!("{}.reference_date", self.path))?,
            },
        };
        let mut group_by = Vec::new();
        for (index, name) in self.groups.iter().enumerate() {
            group_by.push(resolve(
                name,
                format!("{}.window.group_by[{index}]", self.path),
            )?);
        }
        let mut order_by = Vec::new();
        for (index, (name, descending, nulls_first)) in self.order.iter().enumerate() {
            order_by.push(OrderTerm {
                column: resolve(name, format!("{}.window.order_by[{index}]", self.path))?,
                descending: *descending,
                nulls_first: *nulls_first,
            });
        }
        let mut bindings = Vec::new();
        if let Some(plan) = filter {
            for name in plan.identifiers() {
                bindings.push(Binding {
                    name: name.into(),
                    read: Read::Column(resolve(name, plan.spec_path().into())?),
                });
            }
        }
        if findings.len() != start {
            return Ok(None);
        }
        let filter = filter
            .map(|plan| BoundPredicate::new(plan.clone(), bindings))
            .transpose()
            .map_err(BindError::InvalidPredicateBinding)?;
        Ok(Some(Window {
            kind,
            group_by,
            order_by,
            filter,
        }))
    }
}
