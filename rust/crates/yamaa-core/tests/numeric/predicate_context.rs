//! Owned and borrowed resolution preserve independent truth and occurrence effects.
use std::{collections::BTreeMap, convert::Infallible, rc::Rc};
use yamaa_core::{
    predicate::{ErrorKind, Resolved, Resolver, Truth},
    predicate_compiler,
    table::ValueRef,
    value::{Selection, Value},
};
struct Borrowed {
    values: BTreeMap<String, Value>,
    trace: Vec<String>,
}
impl Resolver for Borrowed {
    type Error = Infallible;
    fn resolve(&mut self, id: &str) -> Result<Selection, Self::Error> {
        unreachable!("borrowed port only: {id}")
    }
    fn resolve_value(&mut self, id: &str) -> Result<Resolved<'_>, Self::Error> {
        self.trace.push(id.into());
        Ok(self
            .values
            .get(id)
            .map_or(Resolved::Absent, |v| Resolved::Borrowed(ValueRef::from(v))))
    }
}
struct Owned {
    values: BTreeMap<String, Value>,
    trace: Vec<String>,
    fail: Option<(Box<u8>, Rc<()>)>,
}
impl Resolver for Owned {
    type Error = (Box<u8>, Rc<()>);
    fn resolve(&mut self, id: &str) -> Result<Selection, Self::Error> {
        self.trace.push(id.into());
        if let Some(error) = self.fail.take() {
            return Err(error);
        }
        Ok(self
            .values
            .get(id)
            .cloned()
            .map_or(Selection::Absent, Selection::Present))
    }
}
fn owned(values: &[(&str, Value)]) -> Owned {
    Owned {
        values: values
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
        trace: vec![],
        fail: None,
    }
}
#[test]
fn owned_and_borrowed_ports_preserve_truth_and_written_occurrences() {
    for (expr, values, truth, names) in [
        (
            "a = 0 AND b = 1",
            vec![("a", Value::Int(2)), ("b", Value::Int(1))],
            Truth::False,
            vec!["a", "b"],
        ),
        (
            "a = 2 OR b = 1",
            vec![("a", Value::Int(2)), ("b", Value::Missing)],
            Truth::True,
            vec!["a", "b"],
        ),
        (
            "a BETWEEN b AND c",
            vec![
                ("a", Value::Int(3)),
                ("b", Value::Int(1)),
                ("c", Value::Int(5)),
            ],
            Truth::True,
            vec!["a", "b", "a", "c"],
        ),
        (
            "str_contains(a, 'a.*')",
            vec![("a", Value::Str("baaa".into()))],
            Truth::True,
            vec!["a"],
        ),
        (
            "a LIKE 'a%'",
            vec![("a", Value::Str("abc".into()))],
            Truth::True,
            vec!["a"],
        ),
    ] {
        let plan = predicate_compiler::compile(expr, "columns.F.flag.condition").unwrap();
        let mut new = owned(&values);
        let mut old = Borrowed {
            values: new.values.clone(),
            trace: vec![],
        };
        assert_eq!(plan.evaluate(&mut old).unwrap(), truth);
        assert_eq!(plan.evaluate(&mut new).unwrap(), truth);
        assert_eq!(old.trace, names);
        assert_eq!(new.trace, names);
    }
}
#[test]
fn fixed_regex_steps_preserve_the_actual_resolution_prefix_and_fresh_retry() {
    let plan =
        predicate_compiler::compile("str_contains(a, 'z') AND b = 1", "flag.condition").unwrap();
    let mut port = owned(&[("a", Value::Str("a".repeat(500_000))), ("b", Value::Int(1))]);
    assert!(matches!(
        plan.evaluate(&mut port).unwrap_err().kind,
        ErrorKind::Regex(yamaa_core::regex::MatchError {
            resource: yamaa_core::regex::Resource::Work,
            limit: 1_000_000
        })
    ));
    assert_eq!(port.trace, ["a"]);
    port.values.insert("a".into(), Value::Str("z".into()));
    port.trace.clear();
    assert_eq!(plan.evaluate(&mut port).unwrap(), Truth::True);
    assert_eq!(port.trace, ["a", "b"]);
}
#[test]
fn opaque_error_moves_once_without_later_resolution_and_retains_site_route() {
    let plan = predicate_compiler::compile("a = 1 OR b = 2", "flag.condition").unwrap();
    let marker = Rc::new(());
    let payload = Box::new(9);
    let address = &*payload as *const u8;
    let mut ctx = owned(&[]);
    ctx.fail = Some((payload, marker.clone()));
    let error = plan.evaluate(&mut ctx).unwrap_err();
    assert_eq!(error.spec_path, "flag.condition");
    assert_eq!(error.expression, "a = 1 OR b = 2");
    let ErrorKind::Resolution {
        identifier,
        error: (payload, retained),
    } = error.kind
    else {
        panic!("opaque")
    };
    assert_eq!(identifier, "a");
    assert_eq!(&*payload as *const u8, address);
    assert!(Rc::ptr_eq(&marker, &retained));
    assert_eq!(ctx.trace, ["a"]);
}
