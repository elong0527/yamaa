pub use crate::support::document::Tree;
use crate::support::roots::{Case, Definition, Parameter};
use yamaa_core::{
    ast::{self, Surface},
    project_environment::{Draft, Environment, ExecutionEnvironment},
    project_function::{Function, Language},
    schema::Document,
    value::{ColumnType, Value, ValueType},
};

pub fn function(name: &str) -> Function {
    Function::admit(Language::Python, definition(name).root(Language::Python)).unwrap()
}
/// An integer identity named `name`, with the tests its contract needs.
pub fn definition(name: &str) -> Definition {
    Definition {
        name: name.into(),
        function: format!("project.{name}"),
        description: "Integer identity.".into(),
        params: vec![Parameter {
            name: "x".into(),
            kind: ValueType::Int,
            required: true,
            default: None,
            accepts_missing: false,
        }],
        returns: ColumnType::Int,
        may_return_missing: false,
        comparison_decimals: 4,
        tests: vec![
            Case {
                id: "normal".into(),
                covers: vec!["normal".into()],
                args: vec![("x".into(), Value::Int(7))],
                result: Value::Int(7),
            },
            Case {
                id: "boundary".into(),
                covers: vec!["boundary".into()],
                args: vec![("x".into(), Value::Int(i64::MIN))],
                result: Value::Int(i64::MIN),
            },
            Case {
                id: "missing".into(),
                covers: vec!["short-circuit-missing:x".into()],
                args: vec![("x".into(), Value::Missing)],
                result: Value::Missing,
            },
        ],
    }
}
/// The root of the 1.0 function file defining [`definition`]'s integer
/// identity `name`, implemented in Python (D8).
pub fn file(name: &str) -> ast::Function {
    let callable = format!("project.{name}");
    definition(name).file(&[(Language::Python, &callable)])
}
/// The admitted metadata of a 1.0 project whose function files define the
/// integer identities `names`, in that order.
#[allow(dead_code)]
pub fn environment(names: &[&str]) -> ExecutionEnvironment {
    project(names.iter().map(|name| file(name)).collect())
}
/// The admitted metadata of a 1.0 project whose function files are
/// `functions`, in that order, run by the Python host.
pub fn project(functions: Vec<ast::Function>) -> ExecutionEnvironment {
    Environment::admit(
        Some(Language::Python),
        Draft {
            surface: Surface::V1 { minor: 0 },
            language: Some(Language::Python),
            lock: None,
            functions: Some(functions),
            codelists: vec![],
        },
    )
    .unwrap()
    .into_execution()
}
pub fn call(name: &str, args: Vec<(&str, Tree)>) -> Tree {
    Tree::map(vec![(
        "function",
        Tree::map(vec![("name", Tree::text(name)), ("args", Tree::map(args))]),
    )])
}
/// A 1.0 specification (D4) whose `ID` column reads `SRC.ID` and whose
/// other `columns` derive the expressions given, each written in 1.0.
#[allow(dead_code)]
pub fn specification(columns: Vec<(&str, &str, Tree)>) -> Document {
    specification_with_rows(columns, Vec::new())
}
pub fn specification_with_rows(columns: Vec<(&str, &str, Tree)>, rows: Vec<Tree>) -> Document {
    specification_with_verifications(columns, rows, Vec::new(), None)
}
#[allow(dead_code)]
pub fn specification_with_verifications(
    columns: Vec<(&str, &str, Tree)>,
    rows: Vec<Tree>,
    verifications: Vec<Tree>,
    report: Option<&str>,
) -> Document {
    tree(columns, rows, verifications, report, false).document()
}
/// The specification tree; `legacy` writes the 0.5 header and the 0.5
/// spelling of the `ID` column's read.
fn tree(
    columns: Vec<(&str, &str, Tree)>,
    rows: Vec<Tree>,
    verifications: Vec<Tree>,
    report: Option<&str>,
    legacy: bool,
) -> Tree {
    let mut projection = vec![Tree::text("ID")];
    let read = if legacy {
        Tree::map(vec![(
            "source",
            Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
        )])
    } else {
        Tree::text("SRC.ID")
    };
    let mut declarations = vec![Tree::map(vec![
        ("name", Tree::text("ID")),
        ("type", Tree::text("int")),
        ("derivation", Tree::map(vec![("value", read)])),
    ])];
    for (name, kind, value) in columns {
        projection.push(Tree::text(name));
        declarations.push(Tree::map(vec![
            ("name", Tree::text(name)),
            ("type", Tree::text(kind)),
            ("derivation", Tree::map(vec![("value", value)])),
        ]));
    }
    let mut output = vec![
        ("path", Tree::text("output.csv")),
        ("columns", Tree::List(projection)),
    ];
    if let Some(path) = report {
        output.push(("report", Tree::text(path)));
    }
    let mut fields = if legacy {
        vec![("schema_version", Tree::text("1.0"))]
    } else {
        vec![("yamaa", Tree::text("1.0")), ("type", Tree::text("spec"))]
    };
    fields.extend([
        ("domain", Tree::text("TEST")),
        (
            "input",
            Tree::map(vec![(
                "SRC",
                Tree::map(vec![("path", Tree::text("input.csv"))]),
            )]),
        ),
        ("keys", Tree::List(vec![Tree::text("ID")])),
        ("columns", Tree::List(declarations)),
        ("output", Tree::map(output)),
    ]);
    if !rows.is_empty() {
        fields.push(("rows", Tree::List(rows)));
    }
    if !verifications.is_empty() {
        fields.push(("verifications", Tree::List(verifications)));
    }
    Tree::map(fields)
}
/// The 0.5 specifications of the families that keep their 0.5 documents
/// until the 0.5 path is deleted (#3447): the same tree under the 0.5
/// header, admitted as the 0.5 model; the columns given are written in 0.5.
#[allow(dead_code)]
pub mod v0_5 {
    use super::Tree;
    use yamaa_core::schema::SpecificationDocument;

    pub fn specification(columns: Vec<(&str, &str, Tree)>) -> SpecificationDocument {
        specification_with_rows(columns, Vec::new())
    }
    pub fn specification_with_rows(
        columns: Vec<(&str, &str, Tree)>,
        rows: Vec<Tree>,
    ) -> SpecificationDocument {
        let tree = super::tree(columns, rows, Vec::new(), None, true);
        SpecificationDocument::admit(tree.document())
            .unwrap()
            .unwrap()
    }
}
