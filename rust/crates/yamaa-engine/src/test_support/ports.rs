//! Single-consumption scripted outcomes for the actual application port traits.
use super::*;
use alloc::{collections::VecDeque, sync::Arc};
use function_invocation::{
    Argument, FunctionPort, HostError, InvocationPlan, ProjectInvocationPlan,
};
use output::{ArtifactEncoder, ArtifactPort};
use run::{SourceDecoder, SourcePort};

pub struct InspectionStep<E> {
    pub source: SourceDeclaration,
    pub outcome: Result<(), E>,
}
pub struct CaptureStep<E> {
    pub source: SourceDeclaration,
    pub byte_limit: usize,
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
impl<E> SourcePort for ScriptedSource<E> {
    type Error = E;
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
    fn capture(&mut self, source: &SourceDeclaration, byte_limit: usize) -> Result<Arc<[u8]>, E> {
        self.log.record(Event::Capture {
            source: source.clone(),
            byte_limit,
        });
        let step = self
            .captures
            .pop_front()
            .expect("unexpected source capture");
        assert_eq!(&step.source, source, "capture declaration");
        assert_eq!(step.byte_limit, byte_limit, "capture budget");
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
    fn decode(&mut self, source: &SourceDeclaration, bytes: &[u8]) -> Result<T, E> {
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

pub struct EncodeStep<E> {
    pub projection: Vec<usize>,
    pub byte_limit: usize,
    pub outcome: Result<Vec<u8>, E>,
}
pub struct ScriptedEncoder<E> {
    pub steps: VecDeque<EncodeStep<E>>,
    log: EventLog,
}
impl<E> ScriptedEncoder<E> {
    pub fn new(log: EventLog) -> Self {
        Self {
            steps: VecDeque::new(),
            log,
        }
    }
}
impl<E> ArtifactEncoder for ScriptedEncoder<E> {
    type Error = E;
    fn encode(
        &mut self,
        _: &dataset::Dataset,
        projection: &[usize],
        byte_limit: usize,
    ) -> Result<Vec<u8>, E> {
        self.log.record(Event::Encode {
            projection: projection.into(),
            byte_limit,
        });
        let step = self
            .steps
            .pop_front()
            .expect("unexpected artifact encoding");
        assert_eq!(step.projection, projection, "encoding projection");
        assert_eq!(step.byte_limit, byte_limit, "encoding budget");
        step.outcome
    }
}

pub struct PublishStep<E> {
    pub path: String,
    pub bytes: Vec<u8>,
    pub outcome: Result<(), E>,
}
pub struct ScriptedPublisher<E> {
    pub steps: VecDeque<PublishStep<E>>,
    log: EventLog,
}
impl<E> ScriptedPublisher<E> {
    pub fn new(log: EventLog) -> Self {
        Self {
            steps: VecDeque::new(),
            log,
        }
    }
}
impl<E> ArtifactPort for ScriptedPublisher<E> {
    type Error = E;
    fn publish(&mut self, path: &str, bytes: &[u8]) -> Result<(), E> {
        self.log.record(Event::Publish {
            path: path.into(),
            bytes: bytes.into(),
        });
        let step = self
            .steps
            .pop_front()
            .expect("unexpected artifact publication");
        assert_eq!(step.path, path, "publication path");
        assert_eq!(step.bytes, bytes, "publication bytes");
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
