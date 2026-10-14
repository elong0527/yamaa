//! Independent equality and occurrence/effect traces for the shared interpreter.
use std::{collections::BTreeMap, convert::Infallible, rc::Rc};
use yamaa_core::{
    predicate::{
        Budget, ContextError, ErrorKind, EvaluationContext, Resolved, Resolver, Resource, Truth,
        Usage,
    },
    predicate_compiler,
    table::ValueRef,
    value::{Selection, Value},
};
struct Old {
    values: BTreeMap<String, Value>,
    trace: Vec<String>,
}
impl Resolver for Old {
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
struct Context {
    values: BTreeMap<String, Value>,
    trace: Vec<String>,
    budget: Budget,
    fail: Option<(Box<u8>, Rc<()>)>,
}
impl EvaluationContext for Context {
    type Error = (Box<u8>, Rc<()>);
    fn budget(&mut self) -> &mut Budget {
        &mut self.budget
    }
    fn resolve(
        &mut self,
        id: &str,
        local: &mut Budget,
    ) -> Result<Selection, ContextError<Self::Error>> {
        self.trace.push(id.into());
        if let Some(error) = self.fail.take() {
            return Err(ContextError::Resolution(error));
        }
        let Some(value) = self.values.get(id) else {
            return Ok(Selection::Absent);
        };
        self.budget
            .operand_text(
                local,
                match value {
                    Value::Str(s) => s.len(),
                    _ => 0,
                },
            )
            .map_err(ContextError::Limit)?;
        Ok(Selection::Present(value.clone()))
    }
}
fn usage() -> Usage {
    yamaa_core::predicate::Limits::default().into()
}
fn context(values: &[(&str, Value)], usage: Usage) -> Context {
    Context {
        values: values
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
        trace: vec![],
        budget: Budget::new(usage),
        fail: None,
    }
}
#[test]
fn old_and_context_ports_share_truth_full_usage_and_written_occurrences() {
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
        let plan =
            predicate_compiler::compile(expr, "columns.F.flag.condition", Default::default())
                .unwrap();
        let mut new = context(&values, usage());
        let mut old = Old {
            values: new.values.clone(),
            trace: vec![],
        };
        let mut budget = Budget::new(usage());
        assert_eq!(
            plan.evaluate_with_budget(&mut old, &mut budget).unwrap(),
            truth
        );
        assert_eq!(plan.evaluate_in_context(&mut new).unwrap(), truth);
        assert_eq!(old.trace, names);
        assert_eq!(new.trace, names);
        assert_eq!(new.budget.used(), budget.used());
    }
}
#[test]
fn cumulative_work_resolution_text_and_regex_failures_preserve_effect_prefixes() {
    let plan =
        predicate_compiler::compile("a = 'text' AND b = 1", "flag.condition", Default::default())
            .unwrap();
    for (usage, resource, trace) in [
        (Usage { work: 0, ..usage() }, Resource::Work, vec![]),
        (
            Usage {
                resolutions: 1,
                ..usage()
            },
            Resource::Resolutions,
            vec!["a"],
        ),
        (
            Usage {
                text_bytes: 3,
                ..usage()
            },
            Resource::TextBytes,
            vec!["a"],
        ),
    ] {
        let mut ctx = context(
            &[("a", Value::Str("text".into())), ("b", Value::Int(1))],
            usage,
        );
        assert!(
            matches!(plan.evaluate_in_context(&mut ctx).unwrap_err().kind,ErrorKind::Limit(limit) if limit.resource==resource)
        );
        assert_eq!(ctx.trace, trace);
    }
    let plan = predicate_compiler::compile(
        "str_contains(a,'a') AND b = 1",
        "flag.condition",
        Default::default(),
    )
    .unwrap();
    let mut ctx = context(
        &[("a", Value::Str("a".into())), ("b", Value::Int(1))],
        Usage {
            regex: yamaa_core::regex::MatchUsage {
                work: 0,
                ..usage().regex
            },
            ..usage()
        },
    );
    assert!(matches!(
        plan.evaluate_in_context(&mut ctx).unwrap_err().kind,
        ErrorKind::Limit(_)
    ));
    assert_eq!(ctx.trace, ["a"]);
}
#[test]
fn opaque_error_moves_once_without_later_resolution_and_retains_site_route() {
    let plan = predicate_compiler::compile("a = 1 OR b = 2", "flag.condition", Default::default())
        .unwrap();
    let marker = Rc::new(());
    let payload = Box::new(9);
    let address = &*payload as *const u8;
    let mut ctx = context(&[], usage());
    ctx.fail = Some((payload, marker.clone()));
    let error = plan.evaluate_in_context(&mut ctx).unwrap_err();
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
