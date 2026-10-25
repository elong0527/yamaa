pub use crate::support::document::Tree;
use crate::support::roots::{Case, Definition, Parameter};
use yamaa_core::{
    ast,
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
/// [`specification`], whose input declares `ID` as `int`, as a decoder that
/// captures it so does: before capture, an undeclared field of the
/// delimited input is `str` (INPUT-5).
#[allow(dead_code)]
pub fn typed_specification(columns: Vec<(&str, &str, Tree)>) -> Document {
    build(columns, Vec::new(), Vec::new(), None, true)
}
#[allow(dead_code)]
pub fn specification_with_verifications(
    columns: Vec<(&str, &str, Tree)>,
    rows: Vec<Tree>,
    verifications: Vec<Tree>,
    report: Option<&str>,
) -> Document {
    build(columns, rows, verifications, report, false)
}
fn build(
    columns: Vec<(&str, &str, Tree)>,
    rows: Vec<Tree>,
    verifications: Vec<Tree>,
    report: Option<&str>,
    typed: bool,
) -> Document {
    let mut projection = vec![Tree::text("ID")];
    let mut declarations = vec![Tree::map(vec![
        ("name", Tree::text("ID")),
        ("type", Tree::text("int")),
        (
            "derivation",
            Tree::map(vec![("value", Tree::text("SRC.ID"))]),
        ),
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
    let mut fields = vec![("yamaa", Tree::text("1.0")), ("type", Tree::text("spec"))];
    fields.extend([
        ("domain", Tree::text("TEST")),
        (
            "input",
            Tree::map(vec![(
                "SRC",
                Tree::map(
                    std::iter::once(("path", Tree::text("input.csv")))
                        .chain(typed.then(|| ("types", Tree::map(vec![("ID", Tree::text("int"))]))))
                        .collect(),
                ),
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
    Tree::map(fields).document()
}
