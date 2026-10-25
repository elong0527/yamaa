//! The file route's build and save through a scripted host-functions port
//! (#3778 group 2): a study directory of authored files whose project's
//! functions a double of the port answers as the case scripts them, standing
//! in for the host behind the facade's project functions.
//!
//! `host_build` prepares and runs the request as `build` does, through the
//! double, and classifies each failure the run retains as its script says, as
//! a host classifies its own (`file_application::run`); it returns the issues
//! of the result or of its refusal, with the output table and the run report
//! a result holds. `host_save` builds as `host_build` does, then saves the
//! result, as `save` does, and returns its issues afterwards and each file of
//! the study directory it created, changed or removed. Each takes the study a
//! case stages ([`super::study`]), the `specification` and the optional
//! `project` a request names, the `host` it runs for, `python` unless
//! written, and the double's script (`functions`).
//!
//! The script answers the port's three calls:
//! - `lock`, the failure the lock verification raises; without it, the lock
//!   verifies;
//! - `bind`, each function whose binding raises, with its failure; every
//!   other binding succeeds;
//! - `calls`, the reply to each invocation, found by its `function` and its
//!   `args`, each argument's name and value written as a table cell: the
//!   value it `returns`, the failure it `raises`, a result the host could not
//!   represent, with its failure (`rejects`), or a panic (`"unwinds":
//!   true`). An invocation the script gives no reply refuses the input.
//!
//! A failure is a host exception (`class` and `message`), a value the host
//! rejected (`reason` and `returned`), lock findings (`lock`, each with its
//! `package`, `reason`, `expected` and `actual`), each a fact the host
//! classifies it with; `"opaque"`, a failure the host does not classify; or
//! `"interrupt"`, the host's interrupt.
//!
//! The double logs what the port received and where the run starts reading
//! its study input, in order ([`logged`]): `lock <language> <kind>
//! <written>: <function>=<call>, ...` for the lock verification and the
//! functions it names, `bind <function>: <call>(<parameter>, ...)` for a binding,
//! `<function>(<name>=<value>, ...)` for an invocation, each value written as
//! a table cell, and `input` for the study input. The case form holds no
//! call log, so `fixtures/function_call_contracts.json` states each case's,
//! which `case_api_contracts.rs` compares.
use super::{contents, host, path, rows, study, File, Reported, STUDY};
use crate::support::diagnostic::{
    form::{self, Outcome},
    replay::{closed, field, items, object, text},
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
use yamaa_adapters::{
    callback::CallbackError,
    project_lock::{Finding, Reason},
};
use yamaa_application::file_application::{self, Classification};
use yamaa_core::{
    application_observer::{Event, Observer, Phase, Scope},
    domain_entry::Request,
    function_invocation::{Argument, HostError},
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    host_functions::{HostFunctions, Lock},
    json::{Map, Value},
    project_environment::LockKind,
    project_function::Language,
    value::{FiniteFloat, Value as Scalar},
};

pub(super) const FILES: &[File] = &[
    fixture!("project_activation.json"),
    fixture!("project_functions.json"),
];

pub(super) fn run(operation: &str, input: &Value) -> Option<Reported> {
    logged(operation, input).map(|(reported, _)| reported)
}

/// What `operation` reports for `input`, with the calls its port received.
pub(super) fn logged(operation: &str, input: &Value) -> Option<(Reported, Vec<String>)> {
    let saves = match operation {
        "host_build" => false,
        "host_save" => true,
        _ => return None,
    };
    closed(
        input,
        &[
            &STUDY[..],
            &["specification", "project", "host", "functions"],
        ]
        .concat(),
    );
    let study = study(input);
    let project = input.get("project").map(|name| path(&study, text(name)));
    let specification = path(&study, text(field(input, "specification")));
    let request = Request {
        specification: &specification,
        environment: project.as_deref(),
    };
    let log = Log::default();
    let mut port = Port {
        script: Script::read(input.get("functions")),
        log: log.clone(),
        unscripted: Vec::new(),
    };
    // The directory as the case staged it, which what the request did is
    // read against, whether it is refused or not.
    let staged = saves.then(|| contents(&study));
    let (issues, outcome) = match file_application::select(Some(host(input)), request, None) {
        Err(error) => (
            file_application::collect(yamaa_application::project_application::selection_failure(
                &error,
            )),
            Outcome::default(),
        ),
        Ok(prepared) => {
            let mut run = prepared.run_scope(None);
            let identity = crate::support::study::identity("diagnostics", &specification);
            let (_, result) = file_application::run(
                &prepared,
                &mut run,
                &mut port,
                classify,
                identity,
                Scope::new(&log),
            );
            match result {
                Ok(mut domain) => {
                    if saves {
                        // What the save returns follows from its issues and
                        // what it did to the study directory, which the case
                        // states.
                        let _ = domain.save();
                    }
                    let outcome = Outcome {
                        output: domain.output().map(rows),
                        report: Some(rows(domain.run_report())),
                        saved: None,
                    };
                    (domain.issues(), outcome)
                }
                Err(issues) => (issues, Outcome::default()),
            }
        }
    };
    assert!(
        port.unscripted.is_empty(),
        "the script gives no reply to {:?}",
        port.unscripted
    );
    let saved = staged.map(|staged| form::saved(&staged, &contents(&study)));
    let reported = Reported {
        issues,
        study: Some(study),
        outcome: Outcome { saved, ..outcome },
    };
    Some((reported, log.0.take()))
}

/// The calls the port received and the run's study input, in order; the
/// run's observer, which logs where it starts reading its input.
#[derive(Clone, Default)]
struct Log(Rc<RefCell<Vec<String>>>);

impl Log {
    fn push(&self, entry: String) {
        self.0.borrow_mut().push(entry);
    }
}

impl Observer for Log {
    fn observe(&self, event: Event) {
        if let Event::Phase {
            phase: Phase::Input,
        } = event
        {
            self.push("input".into());
        }
    }
}

/// A failure the script has a call raise, with the facts the host classifies
/// it with.
#[derive(Clone, Debug)]
enum Failure {
    Exception {
        class: String,
        message: String,
    },
    Rejected {
        reason: String,
        returned: Option<String>,
    },
    Lock(Vec<(String, Reason, Vec<String>, Option<String>)>),
    Opaque,
    Interrupt,
}

impl Failure {
    fn read(written: &Value) -> Self {
        let optional = |value: &Value| match value {
            Value::Null => None,
            value => Some(text(value).to_owned()),
        };
        match written {
            Value::String(kind) if kind == "opaque" => Self::Opaque,
            Value::String(kind) if kind == "interrupt" => Self::Interrupt,
            Value::Object(members) if members.contains_key("class") => {
                closed(written, &["class", "message"]);
                Self::Exception {
                    class: text(field(written, "class")).to_owned(),
                    message: text(field(written, "message")).to_owned(),
                }
            }
            Value::Object(members) if members.contains_key("reason") => {
                closed(written, &["reason", "returned"]);
                Self::Rejected {
                    reason: text(field(written, "reason")).to_owned(),
                    returned: optional(field(written, "returned")),
                }
            }
            Value::Object(_) => {
                closed(written, &["lock"]);
                let findings = items(field(written, "lock")).iter().map(|finding| {
                    closed(finding, &["package", "reason", "expected", "actual"]);
                    let reason = text(field(finding, "reason"));
                    (
                        text(field(finding, "package")).to_owned(),
                        Reason::parse(reason).unwrap_or_else(|| panic!("no lock reason {reason}")),
                        items(field(finding, "expected"))
                            .iter()
                            .map(|version| text(version).to_owned())
                            .collect(),
                        optional(field(finding, "actual")),
                    )
                });
                Self::Lock(findings.collect())
            }
            other => panic!("a failure is an object, \"opaque\" or \"interrupt\", not {other:?}"),
        }
    }
}

/// What the host classifies a retained failure as: its exception or rejected
/// value, its lock findings, or nothing.
fn classify(failure: &Failure) -> Classification {
    match failure {
        Failure::Exception { class, message } => Classification::Host(CallbackError::Exception {
            class: class.clone(),
            message: message.clone(),
        }),
        Failure::Rejected { reason, returned } => Classification::Host(CallbackError::Rejected {
            reason: reason.clone(),
            returned: returned.clone(),
        }),
        Failure::Lock(findings) => Classification::Lock(
            findings
                .iter()
                .map(|(package, reason, expected, actual)| Finding {
                    package: package.clone(),
                    reason: *reason,
                    expected: expected.clone(),
                    actual: actual.clone(),
                })
                .collect(),
        ),
        Failure::Opaque | Failure::Interrupt => Classification::Opaque,
    }
}

/// The reply to one invocation.
enum Reply {
    Returns(Scalar),
    Raises(Failure),
    Rejects(Failure),
    Unwinds,
}

/// How the double answers each call.
#[derive(Default)]
struct Script {
    lock: Option<Failure>,
    bind: BTreeMap<String, Failure>,
    /// Each invocation's function and arguments, by name, and its reply.
    calls: Vec<(String, Value, Reply)>,
}

impl Script {
    fn read(written: Option<&Value>) -> Self {
        let Some(written) = written else {
            return Self::default();
        };
        closed(written, &["lock", "bind", "calls"]);
        let calls = written.get("calls").map_or(&[][..], items).iter();
        Self {
            lock: written.get("lock").map(Failure::read),
            bind: written
                .get("bind")
                .map(object)
                .into_iter()
                .flatten()
                .map(|(name, failure)| (name.clone(), Failure::read(failure)))
                .collect(),
            calls: calls
                .map(|call| {
                    let members = closed(
                        call,
                        &[
                            "function", "args", "returns", "raises", "rejects", "unwinds",
                        ],
                    );
                    let replies = ["returns", "raises", "rejects", "unwinds"]
                        .into_iter()
                        .filter_map(|reply| members.get(reply).map(|value| (reply, value)))
                        .collect::<Vec<_>>();
                    let reply = match replies.as_slice() {
                        [("returns", value)] => Reply::Returns(scalar(value)),
                        [("raises", failure)] => Reply::Raises(Failure::read(failure)),
                        [("rejects", failure)] => Reply::Rejects(Failure::read(failure)),
                        [("unwinds", Value::Bool(true))] => Reply::Unwinds,
                        _ => panic!("a call returns, raises, rejects or unwinds: {call:?}"),
                    };
                    let args = field(call, "args");
                    object(args);
                    (
                        text(field(call, "function")).to_owned(),
                        args.clone(),
                        reply,
                    )
                })
                .collect(),
        }
    }
}

/// The value a table cell writes.
fn scalar(cell: &Value) -> Scalar {
    match cell {
        Value::Null => Scalar::Missing,
        Value::Bool(value) => Scalar::Bool(*value),
        Value::String(value) => Scalar::Str(value.clone()),
        Value::Number(number) => match number.as_i64() {
            Some(value) => Scalar::Int(value),
            None => Scalar::Float(
                number
                    .as_f64()
                    .and_then(FiniteFloat::new)
                    .unwrap_or_else(|| panic!("a finite number: {number}")),
            ),
        },
        other => panic!("a scalar cell, not {other:?}"),
    }
}

/// The double of the host-functions port: it answers each call as its script
/// says and logs it.
struct Port {
    script: Script,
    log: Log,
    /// Each invocation the script gives no reply to.
    unscripted: Vec<String>,
}

impl HostFunctions for Port {
    /// The bound function's name.
    type Handle = String;
    type Error = Failure;

    fn verify_lock(
        &mut self,
        language: Language,
        lock: Lock<'_>,
        functions: &[ProjectFunctionIdentity],
    ) -> Result<(), Failure> {
        let kind = match lock.reference.kind {
            LockKind::Uv => "uv",
            LockKind::Renv => "renv",
        };
        let names = functions
            .iter()
            .map(|function| format!("{}={}", function.name, function.call));
        self.log.push(format!(
            "lock {} {kind} {}: {}",
            language.key(),
            lock.reference.written,
            names.collect::<Vec<_>>().join(", ")
        ));
        self.script.lock.clone().map_or(Ok(()), Err)
    }

    fn bind(
        &mut self,
        identity: &ProjectFunctionIdentity,
        signature: &LogicalSignature,
    ) -> Result<String, Failure> {
        let parameters = signature.parameters().iter();
        let parameters = parameters.map(|parameter| parameter.name.as_str());
        self.log.push(format!(
            "bind {}: {}({})",
            identity.name,
            identity.call,
            parameters.collect::<Vec<_>>().join(", ")
        ));
        match self.script.bind.get(&identity.name) {
            Some(failure) => Err(failure.clone()),
            None => Ok(identity.name.clone()),
        }
    }

    fn is_interrupt(&self, failure: &Failure) -> bool {
        matches!(failure, Failure::Interrupt)
    }

    fn invoke(
        &mut self,
        function: &String,
        arguments: &[Argument<'_>],
    ) -> Result<Scalar, HostError<Failure>> {
        let cells = arguments
            .iter()
            .map(|argument| (argument.name.to_owned(), form::cell(argument.value)))
            .collect::<Map<_, _>>();
        let written = arguments
            .iter()
            .map(|argument| {
                let cell = yamaa_adapters::json_wire::encode(&cells[argument.name]);
                format!("{}={cell}", argument.name)
            })
            .collect::<Vec<_>>();
        let entry = format!("{function}({})", written.join(", "));
        self.log.push(entry.clone());
        let args = Value::Object(cells);
        let reply = self
            .script
            .calls
            .iter()
            .find(|(name, written, _)| name == function && *written == args);
        match reply.map(|(_, _, reply)| reply) {
            Some(Reply::Returns(value)) => Ok(value.clone()),
            Some(Reply::Raises(failure)) => Err(HostError::Raised(failure.clone())),
            Some(Reply::Rejects(failure)) => Err(HostError::InvalidResult(failure.clone())),
            // The engine contains a panic in a host call as it contains a
            // host's; resuming one skips the panic hook's message.
            Some(Reply::Unwinds) => std::panic::resume_unwind(Box::new("a scripted unwind")),
            None => {
                self.unscripted.push(entry);
                Err(HostError::Raised(Failure::Opaque))
            }
        }
    }
}
