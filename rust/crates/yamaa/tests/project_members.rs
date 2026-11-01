//! A project run's members built at once (#4193): the seams between the
//! threads a project run builds its members on and the calling thread, which
//! the benchmark readers cannot reach, since they compare saved files and
//! run each entry alone. A run on four threads gives the outcome, printed
//! text, issues, reports, saved files and progress events of the same run on
//! one thread; the progress callback, the project functions and the
//! cancellation probe run on the calling thread alone; and a callback that
//! fails, or that cancels the run, hears of no later member, as on one
//! thread.
//!
//! The study is a project of nine specifications on local files, so the
//! engine reads through no caller resolver, each run in a scratch directory.
//! Its run order and statuses are derived from the project file (PROJECT-11):
//! the producer-first order of the listings, which list every producer
//! first; `BAD` fails, its input missing; `AFTER` reads `BAD`'s artifact, so
//! it is skipped; every other member succeeds, `CALLS` through the project
//! function `double`, and `ADCALL` from `CALLS`'s artifact.
#![cfg(feature = "yaml")]

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;

use yamaa::host::functions::{
    Argument, CallError, FunctionError, FunctionHandle, FunctionIdentity, LockContext, Signature,
};
use yamaa::scalar::{Scalar, ScalarRef};
use yamaa::{
    CancellationToken, Engine, EngineOptions, FailureKind, HostLanguage, Partitions, ProgressEvent,
    ProjectFunctions, ProjectRun, Status,
};

use crate::support::Scratch;

/// The members of the study in run order, with the status each run gives.
const STATUSES: [(&str, Status); 9] = [
    ("DM", Status::Succeeded),
    ("AE", Status::Succeeded),
    ("ADSL", Status::Succeeded),
    ("ADAE", Status::Succeeded),
    ("BAD", Status::Failed),
    ("AFTER", Status::Skipped),
    ("CALLS", Status::Succeeded),
    ("ADCALL", Status::Succeeded),
    ("ADTWO", Status::Succeeded),
];

/// A specification with the keys `keys`, reading `input`, deriving
/// `columns` and writing them, with its report, under `out/`.
fn spec(domain: &str, keys: &str, input: &str, columns: &[(&str, &str, &str)]) -> String {
    let lower = domain.to_lowercase();
    let names = columns
        .iter()
        .map(|(name, ..)| *name)
        .collect::<Vec<_>>()
        .join(", ");
    let columns = columns
        .iter()
        .map(|(name, kind, derivation)| {
            format!("  - {{name: {name}, type: {kind}, label: {name}, derivation: {derivation}}}\n")
        })
        .collect::<String>();
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: {domain}\nkeys: [{keys}]\ninput: {{{input}}}\ncolumns:\n{columns}output: {{path: ../out/{lower}.csv, columns: [{names}], report: ../out/{lower}-report.csv}}\n"
    )
}

/// The study's files in a new scratch directory.
fn study() -> Scratch {
    let study = Scratch::new("project-members");
    study.write("raw/dm.csv", "USUBJID,AGE\n01,40\n02,40\n03,52\n");
    study.write("raw/ae.csv", "USUBJID,AESEQ\n01,1\n01,2\n03,1\n");
    let subject = ("USUBJID", "str", "RAW.USUBJID");
    study.write(
        "sdtm/dm.yaml",
        spec(
            "DM",
            "USUBJID",
            "RAW: {path: ../raw/dm.csv, types: {AGE: int}}",
            &[subject, ("AGE", "int", "RAW.AGE")],
        ),
    );
    study.write(
        "sdtm/ae.yaml",
        spec(
            "AE",
            "USUBJID, AESEQ",
            "RAW: {path: ../raw/ae.csv, types: {AESEQ: int}}",
            &[subject, ("AESEQ", "int", "RAW.AESEQ")],
        ),
    );
    let linked = |alias: &str, producer: &str, schema: &str| {
        format!("{alias}: {{path: ../out/{producer}.csv, schema: {schema}}}")
    };
    study.write(
        "adam/adsl.yaml",
        spec(
            "ADSL",
            "USUBJID",
            &linked("DM", "dm", "../sdtm/dm.yaml"),
            &[("USUBJID", "str", "DM.USUBJID"), ("AGE", "int", "DM.AGE")],
        ),
    );
    study.write(
        "adam/adae.yaml",
        spec(
            "ADAE",
            "USUBJID, AESEQ",
            &linked("AE", "ae", "../sdtm/ae.yaml"),
            &[
                ("USUBJID", "str", "AE.USUBJID"),
                ("AESEQ", "int", "AE.AESEQ"),
            ],
        ),
    );
    study.write(
        "adam/bad.yaml",
        spec(
            "BAD",
            "AGE",
            "RAW: {path: ../raw/missing.csv, types: {AGE: int}}",
            &[("AGE", "int", "RAW.AGE")],
        ),
    );
    study.write(
        "adam/after.yaml",
        spec(
            "AFTER",
            "AGE",
            &linked("BAD", "bad", "bad.yaml"),
            &[("AGE", "int", "BAD.AGE")],
        ),
    );
    study.write(
        "adam/calls.yaml",
        spec(
            "CALLS",
            "USUBJID",
            "RAW: {path: ../raw/dm.csv, types: {AGE: int}}",
            &[
                subject,
                (
                    "TWICE",
                    "int",
                    "{function: {name: double, args: {x: RAW.AGE}}}",
                ),
            ],
        ),
    );
    study.write(
        "adam/adcall.yaml",
        spec(
            "ADCALL",
            "USUBJID",
            &linked("CALLS", "calls", "calls.yaml"),
            &[
                ("USUBJID", "str", "CALLS.USUBJID"),
                ("TWICE", "int", "CALLS.TWICE"),
            ],
        ),
    );
    study.write(
        "adam/adtwo.yaml",
        spec(
            "ADTWO",
            "USUBJID",
            &linked("ADSL", "adsl", "adsl.yaml"),
            &[
                ("USUBJID", "str", "ADSL.USUBJID"),
                ("AGE", "int", "ADSL.AGE"),
            ],
        ),
    );
    study.write(
        "functions/double.yaml",
        "yamaa: \"1.0\"\ntype: function\nname: double\ndescription: Double the value.\nparams: [{name: x, type: int}]\nreturns: int\nimplementations: {python: pkg.double}\ntests:\n  - {id: normal, covers: [normal], args: {x: 1}, result: 2}\n  - {id: zero, covers: [boundary], args: {x: 0}, result: 0}\n  - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n",
    );
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nfunctions: [functions/double.yaml]\nsdtm: {specs: [sdtm/dm.yaml, sdtm/ae.yaml]}\nadam: {specs: [adam/adsl.yaml, adam/adae.yaml, adam/bad.yaml, adam/after.yaml, adam/calls.yaml, adam/adcall.yaml, adam/adtwo.yaml]}\n",
    );
    study
}

/// `double`, recording the thread of every call the engine makes to it.
#[derive(Default)]
struct Doubling {
    threads: Mutex<Vec<ThreadId>>,
}

impl Doubling {
    fn called(&self) {
        self.threads
            .lock()
            .unwrap()
            .push(std::thread::current().id());
    }
}

impl ProjectFunctions for Doubling {
    fn verify_lock(
        &self,
        _: &LockContext<'_>,
        _: &[FunctionIdentity],
    ) -> Result<(), FunctionError> {
        self.called();
        Ok(())
    }
    fn bind(&self, _: &FunctionIdentity, _: &Signature) -> Result<FunctionHandle, FunctionError> {
        self.called();
        Ok(FunctionHandle::new(()))
    }
    fn invoke(&self, _: &FunctionHandle, arguments: &[Argument<'_>]) -> Result<Scalar, CallError> {
        self.called();
        Ok(match arguments[0].value {
            ScalarRef::Int(x) => Scalar::Int(2 * x),
            _ => Scalar::Missing,
        })
    }
}

/// One event as the tests compare it, with the thread it was reported on.
type Heard = (String, String, Option<usize>, Option<usize>, ThreadId);

/// What the callback does with each event, given the run's token: `Ok` to
/// go on, an error to fail.
type React = fn(&ProgressEvent, &CancellationToken) -> Result<(), FunctionError>;

/// What one run of the study gives: its outcome, or the kind of failure of
/// a call that gave none; the events its callback heard; and the threads of
/// its project function calls.
#[derive(Debug, PartialEq)]
struct Observed {
    outcome: Result<Outcome, FailureKind>,
    events: Vec<Heard>,
    calls: Vec<ThreadId>,
}

/// What a run gives: its printed text, its issues, its members' statuses,
/// outputs and timed phases, its report and the files its save wrote.
#[derive(Debug, PartialEq)]
struct Outcome {
    summary: String,
    issues: String,
    members: Vec<(String, Status, String, Vec<String>)>,
    report: String,
    saved: BTreeMap<String, String>,
}

/// An engine on `threads` threads with the hosts' partitions, `double`,
/// and a callback that keeps every event in `heard` and then does what
/// `react` says with `token`.
fn engine(
    threads: usize,
    functions: &Arc<Doubling>,
    heard: &Arc<Mutex<Vec<Heard>>>,
    (react, token): (React, &CancellationToken),
) -> Engine {
    let (kept, token) = (Arc::clone(heard), token.clone());
    let partitions = Partitions::by_keys(
        NonZeroUsize::new(2).unwrap(),
        NonZeroUsize::new(624_640).unwrap(),
    )
    .with_whole_table_fallback()
    .with_threads(NonZeroUsize::new(threads).unwrap());
    Engine::new(
        EngineOptions::new(HostLanguage::Python, "project-members".into(), "0".into())
            .with_functions(Arc::clone(functions) as Arc<dyn ProjectFunctions>)
            .with_partitions(partitions)
            .with_progress(move |event: &ProgressEvent| {
                kept.lock().unwrap().push((
                    event.name.clone(),
                    format!("{} {}", event.phase.as_str(), event.kind.as_str()),
                    event.number,
                    event.rows,
                    std::thread::current().id(),
                ));
                react(event, &token)
            }),
    )
}

/// The outcome of `run`, saved, with the study's path written `<study>`.
fn outcome(study: &Scratch, mut run: ProjectRun) -> Outcome {
    let unstudied = |text: String| text.replace(study.to_str().unwrap(), "<study>");
    let members = run
        .members()
        .into_iter()
        .map(|(name, member)| {
            let output = member
                .output()
                .map(|table| format!("{:?}", table.batches()))
                .unwrap_or_default();
            let phases = member
                .timings()
                .entries()
                .iter()
                .map(|timing| format!("{} {:?}", timing.phase.as_str(), timing.outcome))
                .collect();
            (name.to_owned(), member.status(), output, phases)
        })
        .collect();
    let summary = unstudied(run.summary().to_string());
    let issues = unstudied(format!("{:?}", run.diagnostics()));
    let report = unstudied(format!("{:?}", run.report().batches()));
    // A run with a failed member saves its members' reports and fails.
    let _ = run.save(&CancellationToken::new());
    let mut saved = BTreeMap::new();
    if let Ok(entries) = std::fs::read_dir(study.join("out")) {
        for entry in entries {
            let entry = entry.unwrap();
            let text = String::from_utf8(std::fs::read(entry.path()).unwrap()).unwrap();
            saved.insert(
                entry.file_name().to_string_lossy().into_owned(),
                unstudied(text),
            );
        }
    }
    Outcome {
        summary,
        issues,
        members,
        report,
        saved,
    }
}

/// Run the study on `threads` threads, with a callback that does what
/// `react` says, and save it, in a scratch directory of its own.
fn observe(threads: usize, react: React) -> Observed {
    let study = study();
    let functions = Arc::new(Doubling::default());
    let heard = Arc::new(Mutex::new(Vec::new()));
    let token = CancellationToken::new();
    let engine = engine(threads, &functions, &heard, (react, &token));
    let project = study.join("project.yaml");
    let run = engine
        .project(project.to_str().unwrap())
        .cancel_with(&token)
        .run();
    let outcome = run
        .map(|run| outcome(&study, run))
        .map_err(|failure| failure.kind);
    let events = heard.lock().unwrap().clone();
    let calls = functions.threads.lock().unwrap().clone();
    Observed {
        outcome,
        events,
        calls,
    }
}

/// The distinct threads of `threads`, in the order they first appear.
fn distinct(threads: impl IntoIterator<Item = ThreadId>) -> Vec<ThreadId> {
    let mut distinct = Vec::new();
    for thread in threads {
        if !distinct.contains(&thread) {
            distinct.push(thread);
        }
    }
    distinct
}

/// The names the events of `events` give, each once, in order.
fn names(events: &[Heard]) -> Vec<&str> {
    let mut names: Vec<&str> = Vec::new();
    for (name, ..) in events {
        if names.last() != Some(&name.as_str()) {
            names.push(name);
        }
    }
    names
}

#[test]
fn members_built_at_once_give_the_run_on_one_thread() {
    let calling = std::thread::current().id();
    let one = observe(1, |_, _| Ok(()));
    let four = observe(4, |_, _| Ok(()));
    let statuses = one.outcome.as_ref().map(|outcome| {
        outcome
            .members
            .iter()
            .map(|(name, status, ..)| (name.clone(), *status))
            .collect::<Vec<_>>()
    });
    let expected = STATUSES.map(|(name, status)| (name.to_owned(), status));
    assert_eq!(
        statuses,
        Ok(expected.to_vec()),
        "the members of the run on one thread"
    );
    // Every member that builds reports its events in run order, one after
    // another; `AFTER` builds nothing.
    let built = STATUSES
        .iter()
        .filter(|(_, status)| *status != Status::Skipped)
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    assert_eq!(names(&four.events), built, "the members the events name");
    assert_eq!(
        distinct(four.events.iter().map(|event| event.4)),
        [calling],
        "the threads the callback ran on"
    );
    assert_eq!(
        distinct(four.calls.iter().copied()),
        [calling],
        "the threads the project function ran on"
    );
    assert_eq!(four, one, "the run on four threads against the run on one");
}

#[test]
fn a_failed_callback_publishes_no_later_member_on_four_threads_either() {
    // The callback fails on the first event of the member named; the run
    // publishes the members up to it, and fails. `CALLS` runs on the calling
    // thread, and `ADCALL` reads its artifact.
    let up_to_calls = ["DM", "AE", "ADSL", "ADAE", "BAD", "AFTER", "CALLS"];
    let cases: [(&str, React, &[&str]); 2] = [
        (
            "ADAE",
            |event, _| match event.name.as_str() {
                "ADAE" => Err(FunctionError::Protocol),
                _ => Ok(()),
            },
            &["DM", "AE", "ADSL", "ADAE"],
        ),
        (
            "CALLS",
            |event, _| match event.name.as_str() {
                "CALLS" => Err(FunctionError::Protocol),
                _ => Ok(()),
            },
            &up_to_calls,
        ),
    ];
    for (member, react, published) in cases {
        let one = observe(1, react);
        let four = observe(4, react);
        let names = one.outcome.as_ref().map(|outcome| {
            outcome
                .members
                .iter()
                .map(|(name, ..)| name.clone())
                .collect::<Vec<_>>()
        });
        let published = published.iter().map(|name| (*name).to_owned()).collect();
        assert_eq!(
            names,
            Ok(published),
            "the members published, failing on {member}"
        );
        assert_eq!(
            four, one,
            "the run failing on {member} on four threads against the run on one"
        );
    }
}

/// Cancel the run with `token` on the end of the output phase of the member
/// `name`, its last event.
fn cancels(event: &ProgressEvent, name: &str, token: &CancellationToken) {
    if event.name == name && event.phase.as_str() == "output" && event.kind.as_str() == "end" {
        token.cancel();
    }
}

#[test]
fn a_callback_that_cancels_the_run_hears_of_no_later_member() {
    // The callback cancels the run on the last event of the member named, its
    // output's end, after which a run on one thread polls the token and
    // stops: the call is cancelled, and the events stop at that member. Once
    // it also fails the callback, as when the cancelled one raises, the run
    // is still cancelled. On the last member, the run is cancelled too.
    let cases: [(&str, React, &[&str]); 4] = [
        (
            "DM",
            |event, token| {
                cancels(event, "DM", token);
                Ok(())
            },
            &["DM"],
        ),
        (
            "ADAE, then failing",
            |event, token| {
                cancels(event, "ADAE", token);
                match token.is_cancelled() {
                    true => Err(FunctionError::Protocol),
                    false => Ok(()),
                }
            },
            &["DM", "AE", "ADSL", "ADAE"],
        ),
        (
            "CALLS",
            |event, token| {
                cancels(event, "CALLS", token);
                Ok(())
            },
            &["DM", "AE", "ADSL", "ADAE", "BAD", "CALLS"],
        ),
        (
            "ADTWO, the last member",
            |event, token| {
                cancels(event, "ADTWO", token);
                Ok(())
            },
            &[
                "DM", "AE", "ADSL", "ADAE", "BAD", "CALLS", "ADCALL", "ADTWO",
            ],
        ),
    ];
    for (member, react, heard) in cases {
        let one = observe(1, react);
        let four = observe(4, react);
        assert_eq!(
            one.outcome,
            Err(FailureKind::Cancelled),
            "the run on one thread cancelled on {member}"
        );
        assert_eq!(
            names(&one.events),
            heard,
            "the members heard, cancelled on {member}"
        );
        assert_eq!(
            four, one,
            "the run cancelled on {member} on four threads against the run on one"
        );
    }
}

#[test]
fn a_run_on_four_threads_asks_the_probe_on_the_calling_thread_alone() {
    // The probe answers true on its fifth question, before every member has
    // built, wherever that falls: the run is cancelled, with no result.
    let study = study();
    let calling = std::thread::current().id();
    let asked = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&asked);
    let token = CancellationToken::with_probe(move || {
        let mut asked = kept.lock().unwrap();
        asked.push(std::thread::current().id());
        asked.len() >= 5
    });
    let heard = Arc::new(Mutex::new(Vec::new()));
    let engine = engine(
        4,
        &Arc::new(Doubling::default()),
        &heard,
        (|_, _| Ok(()), &token),
    );
    let project = study.join("project.yaml");
    let failure = engine
        .project(project.to_str().unwrap())
        .cancel_with(&token)
        .run()
        .err()
        .expect("a cancelled run has no result");
    assert_eq!(failure.kind, FailureKind::Cancelled, "the cancelled run");
    let asked = asked.lock().unwrap();
    assert_eq!(
        (asked.len() >= 5, distinct(asked.iter().copied())),
        (true, vec![calling]),
        "whether the probe was asked five times, and the threads it was asked on"
    );
    let heard = heard.lock().unwrap();
    assert_eq!(
        distinct(heard.iter().map(|event| event.4)),
        distinct(heard.first().map(|_| calling)),
        "the threads the callback of a cancelled run ran on"
    );
}
