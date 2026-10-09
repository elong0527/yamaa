//! Execute compiler-selected row/default calls without a host-authored plan.
#[path = "../../yamaa-core/tests/support/project_call_compiler.rs"]
mod support;
use std::cell::RefCell;
use support::{call, function, specification_with_rows, Tree};
use yamaa_core::{
    function_signature::{InvocationPlan, ProjectInvocationPlan},
    schema::DocumentNode as N,
    specification::PreparedSpecification,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::{ColumnType, Value},
};
use yamaa_engine::{
    dataset::{DatasetExecution, FunctionBindings, Limits},
    function_invocation::{Argument, HostError},
};
fn literal(value: i64) -> Tree {
    Tree::map(vec![(
        "literal",
        Tree::Scalar(N::Integer(value.to_string())),
    )])
}
fn template(id: &str, key: Tree, second: bool) -> Tree {
    let mut derivations = vec![("ID", key), ("D", call("id", vec![("x", Tree::text("A"))]))];
    if second {
        derivations.push(("B", literal(9)));
    }
    Tree::map(vec![
        ("id", Tree::text(id)),
        (
            "derivations",
            Tree::map(
                derivations
                    .into_iter()
                    .map(|(name, value)| (name, Tree::map(vec![("value", value)])))
                    .collect(),
            ),
        ),
    ])
}
struct Table {
    schema: TableSchema,
    observations: RefCell<Vec<&'static str>>,
}
impl TableAccess for Table {
    type Error = &'static str;
    fn schema(&self) -> &TableSchema {
        self.observations.borrow_mut().push("schema");
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.observations.borrow_mut().push("rows");
        1
    }
    fn cell(&self, _: usize, _: usize) -> Result<ValueRef<'_>, CellError<Self::Error>> {
        self.observations.borrow_mut().push("cell");
        Ok(ValueRef::Int(1))
    }
}
struct Bindings {
    signature: ProjectInvocationPlan,
    inputs: Vec<i64>,
}
impl FunctionBindings for Bindings {
    type Error = &'static str;
    fn signature(&self, _: usize) -> Option<&InvocationPlan> {
        None
    }
    fn project_signature(&self, slot: usize) -> Option<&ProjectInvocationPlan> {
        (slot == 0).then_some(&self.signature)
    }
    fn call(
        &mut self,
        slot: usize,
        args: &[Argument<'_>],
    ) -> Result<Value, HostError<Self::Error>> {
        assert_eq!(slot, 0);
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].name, "x");
        let ValueRef::Int(value) = args[0].value else {
            panic!("exact scalar integer")
        };
        self.inputs.push(value);
        Ok(Value::Int(value))
    }
}

#[test]
fn compiled_row_defaults_execute_in_each_template_order_and_repeat_without_caches() {
    let spec = specification_with_rows(
        vec![
            ("A", "int", call("id", vec![("x", Tree::text("B"))])),
            ("B", "int", call("id", vec![("x", Tree::text("C"))])),
            ("C", "int", literal(7)),
            ("D", "int", literal(0)),
        ],
        vec![
            template(
                "first",
                Tree::map(vec![(
                    "source",
                    Tree::map(vec![("variable", Tree::text("SRC.ID"))]),
                )]),
                false,
            ),
            template("second", literal(2), true),
        ],
    );
    let prepared =
        PreparedSpecification::prepare_with_project(&spec, &[function("unused"), function("id")])
            .unwrap();
    let table = Table {
        schema: TableSchema::new(vec![Column {
            name: "ID".into(),
            kind: ColumnType::Int,
        }])
        .unwrap(),
        observations: RefCell::default(),
    };
    let plan = prepared.bind(&table.schema).unwrap();
    assert_eq!(prepared.called_functions(), [1]);
    let mut bindings = Bindings {
        signature: prepared.project_calls().unwrap().plans()[0].clone(),
        inputs: Vec::new(),
    };
    for _ in 0..2 {
        let result = plan
            .execute_observed_functions(
                &table,
                &[],
                &mut bindings,
                Limits {
                    source_rows: 100,
                    output_rows: 100,
                    output_cells: 1000,
                    key_cells: 1000,
                    work_cells: 10000,
                    scalar_text_bytes: 10000,
                    output_text_bytes: 10000,
                    identity_cells: 10000,
                    identity_text_bytes: 10000,
                },
            )
            .result
            .unwrap();
        assert_eq!(result.dataset.row_count(), 2);
        for (row, values) in [[1, 7, 7, 7, 7], [2, 9, 9, 7, 9]].iter().enumerate() {
            for (column, &value) in values.iter().enumerate() {
                assert_eq!(
                    result.dataset.cell(row, column).unwrap(),
                    ValueRef::Int(value)
                );
            }
        }
    }
    assert_eq!(bindings.inputs, [7, 7, 7, 9, 9, 7, 7, 7, 9, 9]);
    assert!(
        table
            .observations
            .borrow()
            .iter()
            .filter(|&&op| op == "cell")
            .count()
            >= 2
    );
}
