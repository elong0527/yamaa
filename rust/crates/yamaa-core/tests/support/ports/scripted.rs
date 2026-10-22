//! Single-consumption scripted outcomes for the actual application port traits.
use super::*;
use function_invocation::Argument;
use function_invocation::FunctionPort;
use function_invocation::HostError;
use run::{SourceDecoder, SpecificationSourcePort};
use std::{collections::VecDeque, sync::Arc};
use yamaa_core::function_signature::InvocationPlan;
use yamaa_core::function_signature::ProjectInvocationPlan;

pub struct MetadataCaptureStep<E> {
    pub kind: metadata::Kind,
    pub declaring_source: String,
    pub written: String,
    pub outcome: Result<metadata::Reply, E>,
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
impl<E> metadata::ProjectCapturePort for ScriptedMetadata<E> {
    type Error = E;
    fn is_interrupt(&self, error: &E) -> bool {
        (self.interrupt_if)(error)
    }
    fn capture(&mut self, request: metadata::Request<'_>) -> Result<metadata::Reply, E> {
        self.log.record(Event::MetadataCapture {
            kind: request.kind,
            declaring_source: request.declaring_source.into(),
            written: request.written.into(),
        });
        let step = self.steps.pop_front().expect("unexpected metadata capture");
        assert_eq!(step.kind, request.kind, "metadata kind");
        assert_eq!(
            step.declaring_source, request.declaring_source,
            "declaring source"
        );
        assert_eq!(step.written, request.written, "authored path");
        step.outcome
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
}

pub struct InspectionStep<E> {
    pub source: SourceDeclaration,
    pub outcome: Result<(), E>,
}
pub struct CaptureStep<E> {
    pub source: SourceDeclaration,
    pub new_snapshots: usize,
    pub outcome: Result<Arc<[u8]>, E>,
}
pub struct ScriptedSource<E> {
    pub inspections: VecDeque<InspectionStep<E>>,
    pub captures: VecDeque<CaptureStep<E>>,
    log: EventLog,
    reads: usize,
}
impl<E> ScriptedSource<E> {
    pub fn new(log: EventLog) -> Self {
        Self {
            inspections: VecDeque::new(),
            captures: VecDeque::new(),
            log,
            reads: 0,
        }
    }
}
impl<E> SpecificationSourcePort for ScriptedSource<E> {
    type Error = E;
    fn resource_failure(&self, _error: &E) -> Option<yamaa_core::resource::ResourceFailure> {
        None
    }
    fn inspect(&mut self, source: &SourceDeclaration) -> Result<(), E> {
        self.log.record(Event::Inspect(source.clone()));
        let step = self
            .inspections
            .pop_front()
            .expect("unexpected source inspection");
        assert_eq!(&step.source, source, "inspection declaration");
        step.outcome
    }
    fn capture_reads(&self) -> usize {
        self.reads
    }
    fn capture(
        &mut self,
        source: &SourceDeclaration,
        origin: &mut Option<run::SourceOrigin>,
    ) -> Result<Arc<[u8]>, E> {
        *origin = Some(run::SourceOrigin::Resource);
        self.log.record(Event::Capture {
            source: source.clone(),
        });
        let step = self
            .captures
            .pop_front()
            .expect("unexpected source capture");
        assert_eq!(&step.source, source, "capture declaration");
        self.reads = self
            .reads
            .checked_add(step.new_snapshots)
            .expect("fixture snapshot counter");
        step.outcome
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
