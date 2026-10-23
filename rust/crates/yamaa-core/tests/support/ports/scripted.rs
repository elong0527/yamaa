//! Single-consumption scripted outcomes for the actual application port traits.
use super::*;
use function_invocation::Argument;
use function_invocation::FunctionPort;
use function_invocation::HostError;
use run::SourceDecoder;
use std::{collections::VecDeque, sync::Arc};
use yamaa_core::function_signature::InvocationPlan;
use yamaa_core::function_signature::ProjectInvocationPlan;
use yamaa_core::project_environment::LockKind;
use yamaa_core::resolver::{Captured, Failure, File, Located, Name, Resolver};

/// One file a project names: its request, and the held source the resolver
/// locates and captures for it, or the error it fails with when located.
pub struct MetadataCaptureStep<E> {
    pub kind: metadata::Kind,
    pub declaring_source: String,
    pub written: String,
    pub outcome: Result<metadata::Source, E>,
}
pub struct ScriptedMetadata<E> {
    pub steps: VecDeque<MetadataCaptureStep<E>>,
    pub interrupt_if: fn(&E) -> bool,
    log: EventLog,
}
impl<E> ScriptedMetadata<E> {
    pub fn new(log: EventLog) -> Self {
        Self {
            steps: VecDeque::new(),
            interrupt_if: |_| false,
            log,
        }
    }
}
/// Each file is one step, logged once when it is located: a failed step
/// fails the inspection, and a located one is captured next.
impl<E> Resolver for ScriptedMetadata<E> {
    type Error = E;
    fn inspect(&mut self, name: Name<'_>) -> Result<Located, E> {
        let Name::Written {
            file: File::Project(kind),
            declaring,
            written,
        } = name
        else {
            panic!("unexpected metadata request {name:?}")
        };
        self.log.record(Event::MetadataCapture {
            kind,
            declaring_source: declaring.into(),
            written: written.into(),
        });
        let step = self.steps.front().expect("unexpected metadata capture");
        assert_eq!(step.kind, kind, "metadata kind");
        assert_eq!(step.declaring_source, declaring, "declaring source");
        assert_eq!(step.written, written, "authored path");
        match &step.outcome {
            Ok(source) => Ok(Located {
                identity: source.identity.clone(),
                display_path: source.identity.clone(),
            }),
            Err(_) => Err(self.steps.pop_front().unwrap().outcome.err().unwrap()),
        }
    }
    fn capture(&mut self, name: Name<'_>) -> Result<Captured, E> {
        let step = self.steps.pop_front().expect("unexpected metadata capture");
        assert!(
            matches!(name, Name::Written { written, .. } if written == step.written),
            "a capture follows its own inspection"
        );
        let source = step.outcome.ok().expect("a located file");
        Ok(Captured::resource(Arc::from(source.bytes), true))
    }
    fn failure(&self, _: Name<'_>, _: &E) -> Option<Failure> {
        None
    }
    fn is_interrupt(&self, error: &E) -> bool {
        (self.interrupt_if)(error)
    }
    fn rebase(&mut self, _: &Located, _: &Located, _: &str) -> Result<String, E> {
        panic!("a project file is never rebased")
    }
}

#[derive(Debug)]
pub struct MetadataDocument {
    pub source: metadata::Source,
    pub document: yamaa_core::schema::Document,
}
pub struct MetadataDecodeStep<E> {
    pub source: metadata::Source,
    pub target: metadata::DecodeTarget,
    pub outcome: Result<yamaa_core::schema::Document, E>,
}
pub struct ScriptedMetadataDecoder<E> {
    pub steps: VecDeque<MetadataDecodeStep<E>>,
    log: EventLog,
}
impl<E> ScriptedMetadataDecoder<E> {
    pub fn new(log: EventLog) -> Self {
        Self {
            steps: VecDeque::new(),
            log,
        }
    }
}
impl<E> metadata::ProjectDecodePort for ScriptedMetadataDecoder<E> {
    type Error = E;
    type Document = MetadataDocument;
    fn decode(
        &mut self,
        source: metadata::Source,
        target: metadata::DecodeTarget,
    ) -> Result<MetadataDocument, E> {
        self.log.record(Event::MetadataDecode {
            target,
            identity: source.identity.clone(),
            bytes: source.bytes.clone(),
        });
        let step = self.steps.pop_front().expect("unexpected metadata decode");
        assert_eq!(step.source.identity, source.identity, "decode identity");
        assert_eq!(step.source.bytes, source.bytes, "decode snapshot");
        assert_eq!(step.target, target, "structural target");
        step.outcome
            .map(|document| MetadataDocument { source, document })
    }
    fn document(document: &MetadataDocument) -> &yamaa_core::schema::Document {
        &document.document
    }
    fn source(document: &MetadataDocument) -> &metadata::Source {
        &document.source
    }
    /// The scripted lock syntax: held bytes `uv` are a uv lock and `renv` an
    /// renv lock; any other bytes are no lock.
    fn lock(&mut self, source: &metadata::Source) -> Option<LockKind> {
        match source.bytes.as_slice() {
            b"uv" => Some(LockKind::Uv),
            b"renv" => Some(LockKind::Renv),
            _ => None,
        }
    }
}

pub struct InspectionStep<E> {
    pub source: SourceDeclaration,
    pub outcome: Result<(), E>,
}
pub struct CaptureStep<E> {
    pub source: SourceDeclaration,
    /// Whether the capture takes a new snapshot.
    pub created: bool,
    pub outcome: Result<Arc<[u8]>, E>,
}
pub struct ScriptedSource<E> {
    pub inspections: VecDeque<InspectionStep<E>>,
    pub captures: VecDeque<CaptureStep<E>>,
    log: EventLog,
}
impl<E> ScriptedSource<E> {
    pub fn new(log: EventLog) -> Self {
        Self {
            inspections: VecDeque::new(),
            captures: VecDeque::new(),
            log,
        }
    }
}
impl<E> Resolver for ScriptedSource<E> {
    type Error = E;
    fn inspect(&mut self, name: Name<'_>) -> Result<Located, E> {
        let Name::Input(source) = name else {
            panic!("unexpected source request {name:?}")
        };
        self.log.record(Event::Inspect(source.clone()));
        let step = self
            .inspections
            .pop_front()
            .expect("unexpected source inspection");
        assert_eq!(&step.source, source, "inspection declaration");
        step.outcome.map(|()| Located {
            identity: source.path.clone(),
            display_path: source.path.clone(),
        })
    }
    fn capture(&mut self, name: Name<'_>) -> Result<Captured, E> {
        let Name::Input(source) = name else {
            panic!("unexpected source request {name:?}")
        };
        self.log.record(Event::Capture {
            source: source.clone(),
        });
        let step = self
            .captures
            .pop_front()
            .expect("unexpected source capture");
        assert_eq!(&step.source, source, "capture declaration");
        step.outcome
            .map(|bytes| Captured::resource(bytes, step.created))
    }
    fn failure(&self, _: Name<'_>, _: &E) -> Option<Failure> {
        None
    }
    fn rebase(&mut self, _: &Located, _: &Located, _: &str) -> Result<String, E> {
        panic!("an input is never rebased")
    }
}

pub struct DecodeStep<E, T> {
    pub source: SourceDeclaration,
    pub bytes: Vec<u8>,
    pub outcome: Result<T, E>,
}
pub struct ScriptedDecoder<E, T> {
    pub steps: VecDeque<DecodeStep<E, T>>,
    pub continue_after_failure: bool,
    log: EventLog,
}
impl<E, T> ScriptedDecoder<E, T> {
    pub fn new(log: EventLog) -> Self {
        Self {
            steps: VecDeque::new(),
            continue_after_failure: false,
            log,
        }
    }
}
impl<E, T: TableAccess> SourceDecoder for ScriptedDecoder<E, T> {
    type Error = E;
    type Table = T;
    fn continue_after(&self, _: &E) -> bool {
        self.continue_after_failure
    }
    fn decode(
        &mut self,
        source: &SourceDeclaration,
        bytes: &[u8],
        _interpretation: yamaa_core::specification::SourceInterpretation,
        _contract: Option<&yamaa_core::producer_contract::Contract>,
    ) -> Result<T, E> {
        self.log.record(Event::Decode {
            source: source.clone(),
            bytes: bytes.into(),
        });
        let step = self.steps.pop_front().expect("unexpected source decode");
        assert_eq!(&step.source, source, "decode declaration");
        assert_eq!(step.bytes, bytes, "decode bytes");
        step.outcome
    }
}

pub struct CallStep<E> {
    pub slot: usize,
    pub arguments: Vec<(String, Value)>,
    pub outcome: Result<Value, HostError<E>>,
}
pub struct ScriptedFunctions<E> {
    pub legacy: Vec<InvocationPlan>,
    pub project: Vec<ProjectInvocationPlan>,
    pub steps: VecDeque<CallStep<E>>,
    log: EventLog,
}
impl<E> ScriptedFunctions<E> {
    pub fn new(log: EventLog) -> Self {
        Self {
            legacy: Vec::new(),
            project: Vec::new(),
            steps: VecDeque::new(),
            log,
        }
    }
}
fn owned(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Missing => Value::Missing,
        ValueRef::Str(value) => Value::Str(value.into()),
        ValueRef::Int(value) => Value::Int(value),
        ValueRef::Float(value) => Value::Float(value),
        ValueRef::Bool(value) => Value::Bool(value),
        ValueRef::Date(value) => Value::Date(value),
        ValueRef::DateTime(value) => Value::DateTime(value),
    }
}
impl<E> FunctionPort for ScriptedFunctions<E> {
    type Error = E;
    fn signature(&self, slot: usize) -> Option<&InvocationPlan> {
        self.legacy.get(slot)
    }
    fn project_signature(&self, slot: usize) -> Option<&ProjectInvocationPlan> {
        self.project.get(slot)
    }
    fn call(&mut self, slot: usize, arguments: &[Argument<'_>]) -> Result<Value, HostError<E>> {
        let arguments: Vec<_> = arguments
            .iter()
            .map(|argument| (argument.name.into(), owned(argument.value)))
            .collect();
        self.log.record(Event::Call {
            slot,
            arguments: arguments.clone(),
        });
        let step = self.steps.pop_front().expect("unexpected function call");
        assert_eq!(step.slot, slot, "function slot");
        assert!(
            arguments_equal(&step.arguments, &arguments),
            "mapped arguments differ: expected {:?}, observed {:?}",
            step.arguments,
            arguments
        );
        step.outcome
    }
}
