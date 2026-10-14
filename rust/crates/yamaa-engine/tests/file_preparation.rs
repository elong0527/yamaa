//! Typed effect/order witnesses use independent retained parents and native policy.
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
use yamaa_engine::{
    file_preparation as app,
    inheritance::{Source as Identity, SourceError},
    inheritance_capture::CapturePort,
    project_preparation::Source,
};
#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Resolve(Option<String>, String),
    Entry(Option<String>, String, usize),
    Service,
    Canonical(String, String),
    Text(usize),
    Parent(String, String, usize),
    Alias(String, String, usize),
    Base(String),
    Rebase(String, String, String, usize),
}
type Log = Rc<RefCell<Vec<Event>>>;
// These payloads deliberately have no Debug/Clone and are neither Send nor Sync.
struct Fault {
    allocation: Box<u32>,
    marker: Rc<()>,
}
impl Fault {
    fn new() -> Self {
        Self {
            allocation: Box::new(31),
            marker: Rc::new(()),
        }
    }
}
struct Native {
    log: Log,
    entry: Option<Vec<u8>>,
    parents: BTreeMap<String, Vec<u8>>,
    fault: Option<(usize, Fault)>,
    text: usize,
    work: usize,
    text_limit: usize,
    work_limit: usize,
    identity_pointer: Option<*const u8>,
    display_pointer: Option<*const u8>,
}
impl Native {
    fn event(&mut self, event: Event) -> Result<(), Fault> {
        let at = self.log.borrow().len();
        self.log.borrow_mut().push(event);
        if self.fault.as_ref().is_some_and(|(step, _)| *step == at) {
            return Err(self.fault.take().unwrap().1);
        }
        Ok(())
    }
}
impl app::NativePort for Native {
    type Error = Fault;
    fn resolve_entry(&mut self, declaring: Option<&str>, written: &str) -> Result<String, Fault> {
        self.event(Event::Resolve(declaring.map(Into::into), written.into()))?;
        Ok("/study/entry".into())
    }
    fn capture_entry(
        &mut self,
        declaring: Option<&str>,
        written: &str,
        maximum: usize,
    ) -> Result<Vec<u8>, Fault> {
        self.event(Event::Entry(
            declaring.map(Into::into),
            written.into(),
            maximum,
        ))?;
        Ok(self.entry.take().unwrap())
    }
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Identity, SourceError<Fault>> {
        self.event(Event::Canonical(declaring.into(), written.into()))
            .map_err(SourceError::Raised)?;
        let identity = if written.starts_with('b') { "B" } else { "A" }.to_string();
        let display_path = format!("{declaring}/{written}");
        self.identity_pointer = Some(identity.as_ptr());
        self.display_pointer = Some(display_path.as_ptr());
        Ok(Identity {
            identity,
            display_path,
        })
    }
    fn admit_alias_text(&mut self, bytes: usize) -> Result<(), SourceError<Fault>> {
        self.event(Event::Text(bytes))
            .map_err(SourceError::Raised)?;
        if bytes > self.text_limit.saturating_sub(self.text) {
            return Err(SourceError::Raised(Fault::new()));
        }
        self.text += bytes;
        if bytes > self.work_limit.saturating_sub(self.work) {
            return Err(SourceError::Raised(Fault::new()));
        }
        self.work += bytes;
        Ok(())
    }
    fn capture_parent(
        &mut self,
        declaring: &str,
        written: &str,
        maximum: usize,
    ) -> Result<Vec<u8>, SourceError<Fault>> {
        self.event(Event::Parent(declaring.into(), written.into(), maximum))
            .map_err(SourceError::Raised)?;
        let key = if written.starts_with('b') { "B" } else { "A" };
        Ok(self.parents.remove(key).unwrap())
    }
    fn rebase(
        &mut self,
        layer: &Identity,
        entry: &Identity,
        written: &str,
        maximum: usize,
    ) -> Result<String, Fault> {
        self.event(Event::Rebase(
            layer.identity.clone(),
            entry.identity.clone(),
            written.into(),
            maximum,
        ))?;
        Ok(written.into())
    }
    fn accept_alias(
        &mut self,
        declaring: &str,
        written: &str,
        maximum: usize,
    ) -> Result<(), Fault> {
        self.event(Event::Alias(declaring.into(), written.into(), maximum))
    }
    fn select_entry_base(&mut self, identity: &str) -> Result<(), Fault> {
        self.event(Event::Base(identity.into()))
    }
    fn missing_alias() -> Fault {
        Fault::new()
    }
}
struct Prepared {
    source: Source,
    parents: Vec<Source>,
}
impl app::PreparedView for Prepared {
    type Parent = Source;
    fn parents(&self) -> &[Source] {
        &self.parents
    }
    fn parent_source(parent: &Source) -> &Source {
        parent
    }
}
enum Failure {
    Opaque(Fault),
    Parent(Fault),
    Unavailable,
}
struct Documents {
    log: Log,
    written: Vec<String>,
    rejection: Option<Fault>,
    wrong_parent: bool,
    source_pointer: Option<*const u8>,
    display_pointer: Option<*const u8>,
    parent_pointer: Option<*const u8>,
    continuation_probe: bool,
    missing_after_refusal: bool,
}
impl app::DocumentPort<Fault> for Documents {
    type Prepared = Prepared;
    type Error = Failure;
    fn prepare<P: CapturePort<Error = Fault>>(
        &mut self,
        source: Source,
        display: String,
        port: &mut P,
    ) -> Result<Prepared, Failure> {
        self.log.borrow_mut().push(Event::Service);
        assert_eq!(display, "/study/entry");
        self.source_pointer = Some(source.bytes.as_ptr());
        let mut parents = vec![];
        for written in &self.written {
            let identity = match port.canonicalize(&source.identity, written) {
                Ok(source) => source,
                Err(SourceError::Raised(error)) => {
                    if self.continuation_probe {
                        self.missing_after_refusal = matches!(
                            port.capture(
                                &Identity {
                                    identity: "A".into(),
                                    display_path: "held".into()
                                },
                                17
                            ),
                            Err(SourceError::Raised(_))
                        );
                    }
                    return Err(Failure::Parent(error));
                }
                Err(SourceError::Unavailable) => return Err(Failure::Unavailable),
            };
            self.display_pointer = Some(identity.display_path.as_ptr());
            if parents
                .iter()
                .any(|p: &Source| p.identity == identity.identity)
            {
                continue;
            }
            let bytes = port.capture(&identity, 17).map_err(|error| match error {
                SourceError::Raised(error) => Failure::Parent(error),
                SourceError::Unavailable => Failure::Unavailable,
            })?;
            self.parent_pointer = Some(bytes.as_ptr());
            parents.push(Source {
                identity: identity.identity,
                bytes,
            });
        }
        if let Some(error) = self.rejection.take() {
            return Err(Failure::Opaque(error));
        }
        if self.wrong_parent {
            parents.push(Source {
                identity: "ABSENT".into(),
                bytes: vec![],
            });
        }
        Ok(Prepared { source, parents })
    }
}
fn setup(written: &[&str]) -> (Native, Documents, Log) {
    let log = Rc::new(RefCell::new(vec![]));
    (
        Native {
            log: Rc::clone(&log),
            entry: Some(b"original-entry".to_vec()),
            parents: BTreeMap::from([
                ("A".into(), b"original-a".to_vec()),
                ("B".into(), b"b".to_vec()),
            ]),
            fault: None,
            text: 0,
            work: 0,
            text_limit: usize::MAX,
            work_limit: usize::MAX,
            identity_pointer: None,
            display_pointer: None,
        },
        Documents {
            log: Rc::clone(&log),
            written: written.iter().map(|s| (*s).into()).collect(),
            rejection: None,
            wrong_parent: false,
            source_pointer: None,
            display_pointer: None,
            parent_pointer: None,
            continuation_probe: false,
            missing_after_refusal: false,
        },
        log,
    )
}
fn run(
    native: &mut Native,
    documents: &mut Documents,
) -> Result<Prepared, app::Error<Fault, Failure>> {
    app::prepare(native, documents, None, "root.yaml", 16_777_216)
}
#[test]
fn exact_effect_tape_preserves_duplicates_first_capture_and_success_aliases_before_base() {
    let (mut native, mut documents, log) = setup(&["a", "./a", "a", "b"]);
    let root = native.entry.as_ref().unwrap().as_ptr();
    let a = native.parents["A"].as_ptr();
    let b = native.parents["B"].as_ptr();
    let Ok(prepared) = run(&mut native, &mut documents) else {
        panic!()
    };
    assert_eq!(prepared.source.bytes.as_ptr(), root);
    assert_eq!(prepared.parents[0].bytes.as_ptr(), a);
    assert_eq!(prepared.parents[1].bytes.as_ptr(), b);
    assert_eq!(
        prepared
            .parents
            .iter()
            .map(|p| p.identity.as_str())
            .collect::<Vec<_>>(),
        ["A", "B"]
    );
    // /study/entry is 12 bytes. Canonical identity is one byte; display uses '/'.
    assert_eq!(
        *log.borrow(),
        vec![
            Event::Resolve(None, "root.yaml".into()),
            Event::Entry(None, "root.yaml".into(), 16_777_216),
            Event::Service,
            Event::Canonical("/study/entry".into(), "a".into()),
            Event::Text(15),
            Event::Text(14),
            Event::Parent("/study/entry".into(), "a".into(), 17),
            Event::Canonical("/study/entry".into(), "./a".into()),
            Event::Text(17),
            Event::Text(16),
            Event::Canonical("/study/entry".into(), "a".into()),
            Event::Text(15),
            Event::Canonical("/study/entry".into(), "b".into()),
            Event::Text(15),
            Event::Text(14),
            Event::Parent("/study/entry".into(), "b".into(), 17),
            Event::Alias("/study/entry".into(), "a".into(), 10),
            Event::Alias("/study/entry".into(), "./a".into(), 10),
            Event::Alias("/study/entry".into(), "b".into(), 1),
            Event::Base("/study/entry".into()),
        ]
    );
    assert_eq!((native.text, native.work), (106, 106));
}
#[test]
fn declaring_entry_keeps_authored_spelling_and_service_waits_for_entry_capture() {
    let (mut native, mut documents, log) = setup(&[]);
    assert!(app::prepare(
        &mut native,
        &mut documents,
        Some("/meta/original"),
        "../study/root.yaml",
        3
    )
    .is_ok());
    assert_eq!(
        *log.borrow(),
        vec![
            Event::Resolve(Some("/meta/original".into()), "../study/root.yaml".into()),
            Event::Entry(
                Some("/meta/original".into()),
                "../study/root.yaml".into(),
                3
            ),
            Event::Service,
            Event::Base("/study/entry".into())
        ]
    );
}
#[test]
fn every_original_native_fault_moves_opaque_allocation_and_stops_later_effects() {
    let (mut native, mut documents, log) = setup(&["a", "./a", "a", "b"]);
    assert!(run(&mut native, &mut documents).is_ok());
    let tape = log.borrow().clone();
    for (step, event) in tape.iter().enumerate() {
        if *event == Event::Service {
            continue;
        }
        let (mut native, mut documents, log) = setup(&["a", "./a", "a", "b"]);
        let fault = Fault::new();
        let pointer = &*fault.allocation as *const u32;
        let marker = Rc::as_ptr(&fault.marker);
        native.fault = Some((step, fault));
        let error = match run(&mut native, &mut documents) {
            Err(app::Error::Resource(error))
            | Err(app::Error::Preparation(Failure::Parent(error))) => error,
            _ => panic!("native failure"),
        };
        assert_eq!(&*error.allocation as *const u32, pointer);
        assert_eq!(Rc::as_ptr(&error.marker), marker);
        assert_eq!(*log.borrow(), tape[..=step]);
    }
}
#[test]
fn semantic_rejection_preserves_original_error_and_grants_no_alias_or_base_authority() {
    let (mut native, mut documents, log) = setup(&["a", "./a"]);
    let error = Fault::new();
    let pointer = &*error.allocation as *const u32;
    documents.rejection = Some(error);
    let Err(app::Error::Preparation(Failure::Opaque(error))) = run(&mut native, &mut documents)
    else {
        panic!()
    };
    assert_eq!(&*error.allocation as *const u32, pointer);
    assert!(!log
        .borrow()
        .iter()
        .any(|e| matches!(e, Event::Alias(..) | Event::Base(..))));
}
#[test]
fn alias_metadata_exact_and_one_short_limits_keep_failed_text_without_inserting_ledger() {
    for (text_limit, work_limit, success, text, work) in [
        (29, 29, true, 29, 29),
        (28, 29, false, 15, 15),
        (29, 28, false, 29, 15),
    ] {
        let (mut native, mut documents, log) = setup(&["a"]);
        native.text_limit = text_limit;
        native.work_limit = work_limit;
        documents.continuation_probe = true;
        assert_eq!(run(&mut native, &mut documents).is_ok(), success);
        assert_eq!((native.text, native.work), (text, work));
        if !success {
            assert!(documents.missing_after_refusal);
            assert!(!log
                .borrow()
                .iter()
                .any(|e| matches!(e, Event::Parent(..) | Event::Alias(..) | Event::Base(..))));
        }
    }
}
#[test]
fn late_distinct_alias_refusal_keeps_first_admitted_capture_and_no_success_replay() {
    let (mut native, mut documents, log) = setup(&["a", "./a"]);
    native.work_limit = 61;
    assert!(run(&mut native, &mut documents).is_err());
    assert_eq!((native.text, native.work), (62, 46));
    assert_eq!(
        log.borrow()
            .iter()
            .filter(|e| matches!(e, Event::Parent(..)))
            .count(),
        1
    );
    assert!(!log
        .borrow()
        .iter()
        .any(|e| matches!(e, Event::Alias(..) | Event::Base(..))));
}
#[test]
fn existing_policy_accepts_two_hundred_independent_aliases_without_new_count_ceiling() {
    let aliases = (0..200).map(|i| format!("a{i}")).collect::<Vec<_>>();
    let written = aliases.iter().map(String::as_str).collect::<Vec<_>>();
    let (mut native, mut documents, log) = setup(&written);
    let expected = aliases
        .iter()
        .map(|a| 1 + 12 + 1 + a.len() + 12 + a.len() + 1)
        .sum::<usize>();
    native.text_limit = expected;
    native.work_limit = expected;
    assert!(run(&mut native, &mut documents).is_ok());
    assert_eq!((native.text, native.work), (expected, expected));
    assert_eq!(
        log.borrow()
            .iter()
            .filter(|e| matches!(e, Event::Parent(..)))
            .count(),
        1
    );
    assert_eq!(
        log.borrow()
            .iter()
            .filter(|e| matches!(e, Event::Alias(..)))
            .count(),
        200
    );
}
#[test]
fn unknown_retained_catalog_identity_refuses_before_any_later_base_effect() {
    let (mut native, mut documents, log) = setup(&[]);
    documents.wrong_parent = true;
    assert!(matches!(
        run(&mut native, &mut documents),
        Err(app::Error::Resource(_))
    ));
    assert_eq!(
        *log.borrow(),
        vec![
            Event::Resolve(None, "root.yaml".into()),
            Event::Entry(None, "root.yaml".into(), 16_777_216),
            Event::Service
        ]
    );
}

#[test]
fn canonical_identity_and_display_are_returned_without_replacing_original_allocations() {
    let (mut native, mut documents, _) = setup(&["a"]);
    let Ok(prepared) = run(&mut native, &mut documents) else {
        panic!()
    };
    assert_eq!(
        Some(prepared.parents[0].identity.as_ptr()),
        native.identity_pointer
    );
    // The canonical display is consumed by the document service; recording its
    // borrowed address there establishes the exact returned physical allocation.
    assert_eq!(documents.display_pointer, native.display_pointer);
}
