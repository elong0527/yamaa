//! An observable table port and authored predicate text.
pub(crate) use crate::support::table_schema as schema;
use std::cell::RefCell;
use yamaa_core::{
    expression::Comparison,
    table::{CellError, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};

pub(crate) struct Table {
    pub(crate) schema: TableSchema,
    pub(crate) rows: Vec<Vec<Value>>,
    pub(crate) reads: RefCell<Vec<(usize, usize)>>,
    pub(crate) fail: Option<(usize, usize)>,
}
impl TableAccess for Table {
    type Error = &'static str;
    /// Expose the immutable test snapshot schema.
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    /// Expose source cardinality without a cell read.
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    /// Observe reads and inject opaque access failures without altering values.
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.reads.borrow_mut().push((row, column));
        if self.fail == Some((row, column)) {
            return Err(CellError::Access("source failure"));
        }
        self.rows
            .get(row)
            .and_then(|values| values.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}
/// Build an observable normalized source port.
pub(crate) fn table(columns: &[(&str, ColumnType)], rows: Vec<Vec<Value>>) -> Table {
    Table {
        schema: schema(columns),
        rows,
        reads: RefCell::default(),
        fail: None,
    }
}

/// One authored predicate operand, written into the predicate's text.
#[derive(Clone, Debug)]
pub(crate) enum Scalar {
    Literal(Value),
    Identifier(String),
}

/// An authored predicate node; Boolean children index earlier entries.
#[derive(Clone, Debug)]
pub(crate) enum Node {
    Contains {
        value: Scalar,
        pattern: String,
    },
    Boolean(bool),
    Not(usize),
    And(usize, usize),
    Or(usize, usize),
    Compare {
        operator: Comparison,
        left: Scalar,
        right: Scalar,
    },
    IsNull {
        value: Scalar,
        negated: bool,
    },
    In {
        value: Scalar,
        items: Vec<Scalar>,
        negated: bool,
    },
    Between {
        value: Scalar,
        lower: Scalar,
        upper: Scalar,
        negated: bool,
    },
    Like {
        value: Scalar,
        pattern: Scalar,
        escape: Option<char>,
        negated: bool,
    },
}

fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn scalar_text(scalar: &Scalar) -> String {
    match scalar {
        Scalar::Identifier(name) => name.clone(),
        Scalar::Literal(Value::Missing) => "NULL".into(),
        Scalar::Literal(Value::Int(value)) => value.to_string(),
        Scalar::Literal(Value::Float(value)) => format!("{:?}", value.get()),
        Scalar::Literal(Value::Str(value)) => quoted(value),
        Scalar::Literal(Value::Date(value)) => format!("DATE {}", quoted(&value.to_string())),
        Scalar::Literal(Value::DateTime(value)) => {
            format!("DATETIME {}", quoted(&value.to_string()))
        }
        Scalar::Literal(Value::Bool(_)) => panic!("a truth value is not an operand"),
    }
}

/// Write an authored arena as the predicate text it stands for.
pub(crate) fn predicate_text(nodes: &[Node], root: usize) -> String {
    let negation = |negated: &bool| if *negated { "NOT " } else { "" };
    match &nodes[root] {
        Node::Contains { value, pattern } => {
            format!("str_contains({}, {})", scalar_text(value), quoted(pattern))
        }
        Node::Boolean(value) => if *value { "TRUE" } else { "FALSE" }.into(),
        Node::Not(child) => format!("NOT ({})", predicate_text(nodes, *child)),
        Node::And(left, right) => format!(
            "({}) AND ({})",
            predicate_text(nodes, *left),
            predicate_text(nodes, *right)
        ),
        Node::Or(left, right) => format!(
            "({}) OR ({})",
            predicate_text(nodes, *left),
            predicate_text(nodes, *right)
        ),
        Node::Compare {
            operator,
            left,
            right,
        } => format!(
            "{} {} {}",
            scalar_text(left),
            match operator {
                Comparison::Equal => "=",
                Comparison::NotEqual => "<>",
                Comparison::Less => "<",
                Comparison::LessEqual => "<=",
                Comparison::Greater => ">",
                Comparison::GreaterEqual => ">=",
            },
            scalar_text(right)
        ),
        Node::IsNull { value, negated } => {
            format!("{} IS {}NULL", scalar_text(value), negation(negated))
        }
        Node::In {
            value,
            items,
            negated,
        } => format!(
            "{} {}IN ({})",
            scalar_text(value),
            negation(negated),
            items.iter().map(scalar_text).collect::<Vec<_>>().join(", ")
        ),
        Node::Between {
            value,
            lower,
            upper,
            negated,
        } => format!(
            "{} {}BETWEEN {} AND {}",
            scalar_text(value),
            negation(negated),
            scalar_text(lower),
            scalar_text(upper)
        ),
        Node::Like {
            value,
            pattern,
            escape,
            negated,
        } => format!(
            "{} {}LIKE {}{}",
            scalar_text(value),
            negation(negated),
            scalar_text(pattern),
            escape
                .map(|escape| format!(" ESCAPE {}", quoted(&escape.to_string())))
                .unwrap_or_default()
        ),
    }
}
