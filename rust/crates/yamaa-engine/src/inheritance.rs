//! Demand-driven inheritance traversal; filesystem identity and bytes remain host ports.
use alloc::{string::String, vec, vec::Vec};
use yamaa_core::schema::{
    Document, DocumentNode as N, NormalizationBudget, NormalizationError, SchemaStructure,
};

/// The canonical identity is compared directly. Display spelling retains read-error context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub identity: String,
    pub display_path: String,
}

/// Missing/unreadable/non-file sources have a portable meaning; other host errors stay opaque.
#[derive(Debug, PartialEq, Eq)]
pub enum SourceError<E> {
    Unavailable,
    Raised(E),
}

/// Synchronous source authority. No discovery, retry, concurrency or implicit ambient reader.
pub trait SourcePort {
    type Error;
    /// Resolve relative to the declaring canonical file and verify a regular local file.
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Source, SourceError<Self::Error>>;
    /// Read/decode only when requested. Shared schema admission remains in this service.
    fn read(&mut self, source: &Source) -> Result<Document, SourceError<Self::Error>>;
}

/// A normalized contribution, retained in deterministic postorder with its source identity.
#[derive(Debug, PartialEq)]
pub struct Layer {
    pub source: Source,
    pub document: Document,
}

/// Logical request policies; host allocation/IO containment is a separate deployment concern.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub layers: usize,
    pub parent_visits: usize,
    pub depth: usize,
    pub path_bytes: usize,
    pub input_nodes: usize,
    pub input_text_bytes: usize,
    pub work: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            layers: 256,
            parent_visits: 8192,
            depth: 64,
            path_bytes: 1_048_576,
            input_nodes: 131_072,
            input_text_bytes: 8_388_608,
            work: 8_388_608,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Layers,
    ParentVisits,
    Depth,
    PathBytes,
    InputNodes,
    InputTextBytes,
    Work,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exhausted {
    pub resource: Resource,
    pub limit: usize,
}

/// Reuse this budget with the normalization budget across attempts; failure retains charges.
pub struct Budget {
    limits: Limits,
    used: [usize; 7],
}
impl Budget {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            used: [0; 7],
        }
    }
    pub fn used(&self, resource: Resource) -> usize {
        self.used[resource as usize]
    }
    fn charge(&mut self, resource: Resource, count: usize) -> Result<(), Exhausted> {
        let limit = match resource {
            Resource::Layers => self.limits.layers,
            Resource::ParentVisits => self.limits.parent_visits,
            Resource::Depth => self.limits.depth,
            Resource::PathBytes => self.limits.path_bytes,
            Resource::InputNodes => self.limits.input_nodes,
            Resource::InputTextBytes => self.limits.input_text_bytes,
            Resource::Work => self.limits.work,
        };
        let value = &mut self.used[resource as usize];
        let Some(total) = value.checked_add(count) else {
            *value = usize::MAX;
            return Err(Exhausted { resource, limit });
        };
        *value = total;
        if *value > limit {
            Err(Exhausted { resource, limit })
        } else {
            Ok(())
        }
    }
    fn text(&mut self, value: &str) -> Result<(), Exhausted> {
        self.charge(Resource::PathBytes, value.len())
    }
    fn input(&mut self, input: &Document) -> Result<(), Exhausted> {
        self.charge(Resource::InputNodes, input.nodes().len())?;
        for node in input.nodes() {
            self.charge(Resource::Work, 1)?;
            if let N::Text(s) | N::Integer(s) = node {
                self.charge(Resource::InputTextBytes, s.len())?;
            }
        }
        Ok(())
    }
    fn depth(&mut self, depth: usize) -> Result<(), Exhausted> {
        self.used[Resource::Depth as usize] = self.used[Resource::Depth as usize].max(depth);
        if depth > self.limits.depth.min(128) {
            Err(Exhausted {
                resource: Resource::Depth,
                limit: self.limits.depth.min(128),
            })
        } else {
            Ok(())
        }
    }
}

/// No partial contribution list escapes a traversal failure.
#[derive(Debug, PartialEq)]
pub enum Error<E> {
    Host(E),
    Resource(Exhausted),
    Unavailable {
        declaring: String,
        path: String,
    },
    InvalidParent {
        declaring: String,
        path: String,
    },
    Cycle {
        path: Vec<String>,
        returns_to_entry: bool,
    },
    Version {
        source: String,
        entry: String,
        expected: String,
        actual: String,
        is_entry: bool,
    },
    Layer {
        source: String,
        entry: String,
        input: Document,
        error: NormalizationError,
    },
    /// An admitted custom schema gave control fields a shape this application cannot use.
    UnsupportedControlField {
        source: String,
        field: &'static str,
    },
    InvalidIdentity,
}
impl<E> From<Exhausted> for Error<E> {
    fn from(error: Exhausted) -> Self {
        Self::Resource(error)
    }
}

struct Frame {
    layer: Layer,
    parents: Vec<String>,
    next: usize,
}

/// URI syntax is rejected before granting filesystem authority. Drive-rooted paths stay local.
pub fn is_local_parent(value: &str) -> bool {
    let b = value.as_bytes();
    if b.is_empty() {
        return false;
    }
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'/' | b'\\') {
        return true;
    }
    if !b[0].is_ascii_alphabetic() {
        return true;
    }
    for &c in &b[1..] {
        if c == b':' {
            return false;
        }
        if !(c.is_ascii_alphanumeric() || matches!(c, b'+' | b'.' | b'-')) {
            break;
        }
    }
    true
}

fn text_field<'a>(document: &'a Document, name: &str) -> Option<&'a str> {
    document
        .field(document.root(), name)
        .and_then(|node| match &document.nodes()[node] {
            N::Text(value) => Some(value.as_str()),
            _ => None,
        })
}

fn admit<E>(
    schema: &SchemaStructure,
    entry: &str,
    source: Source,
    input: Document,
    normalization: &mut NormalizationBudget,
    budget: &mut Budget,
) -> Result<Frame, Error<E>> {
    budget.input(&input)?;
    let normalized = match schema.normalize_layer(&input, normalization) {
        Ok(value) => value.document,
        Err(error) => {
            budget.text(entry)?;
            return Err(Error::Layer {
                source: source.identity,
                entry: entry.into(),
                input,
                error,
            });
        }
    };
    budget.charge(Resource::Work, normalized.nodes().len())?;
    let Some(version) = text_field(&normalized, "schema_version") else {
        return Err(Error::UnsupportedControlField {
            source: source.identity,
            field: "schema_version",
        });
    };
    if version != schema.version() {
        budget.text(entry)?;
        budget.text(schema.version())?;
        budget.text(version)?;
        let is_entry = source.identity == entry;
        return Err(Error::Version {
            source: source.identity,
            entry: entry.into(),
            expected: schema.version().into(),
            actual: version.into(),
            is_entry,
        });
    }
    let mut parents = Vec::new();
    if let Some(node) = normalized.field(normalized.root(), "parents") {
        let N::Sequence(items) = &normalized.nodes()[node] else {
            return Err(Error::UnsupportedControlField {
                source: source.identity,
                field: "parents",
            });
        };
        for &child in items {
            let N::Text(parent) = &normalized.nodes()[child] else {
                return Err(Error::UnsupportedControlField {
                    source: source.identity,
                    field: "parents",
                });
            };
            budget.text(parent)?;
            budget.charge(Resource::Work, 1)?;
            parents.push(parent.clone());
        }
    }
    Ok(Frame {
        layer: Layer {
            source,
            document: normalized,
        },
        parents,
        next: 0,
    })
}

/// Visit normalized parents left-to-right, admit each unique layer before reading its parents,
/// and return once-only postorder contributions. The caller supplies the already captured entry.
pub fn traverse<P: SourcePort>(
    schema: &SchemaStructure,
    entry: Source,
    entry_document: Document,
    port: &mut P,
    normalization: &mut NormalizationBudget,
    budget: &mut Budget,
) -> Result<Vec<Layer>, Error<P::Error>> {
    if entry.identity.is_empty() || entry.display_path.is_empty() {
        return Err(Error::InvalidIdentity);
    }
    budget.text(&entry.identity)?;
    budget.text(&entry.display_path)?;
    budget.text(&entry.identity)?;
    let entry_identity = entry.identity.clone();
    budget.depth(1)?;
    budget.charge(Resource::Layers, 1)?;
    let mut active = vec![admit(
        schema,
        &entry_identity,
        entry,
        entry_document,
        normalization,
        budget,
    )?];
    let mut completed: Vec<Layer> = Vec::new();
    while let Some(frame) = active.last_mut() {
        budget.charge(Resource::Work, 1)?;
        if frame.next == frame.parents.len() {
            completed.push(active.pop().expect("active frame").layer);
            continue;
        }
        budget.charge(Resource::ParentVisits, 1)?;
        // Reserve copies before a callback may fail. Each written occurrence is visited.
        let written = &frame.parents[frame.next];
        budget.text(written)?;
        budget.text(&frame.layer.source.identity)?;
        let written = written.clone();
        let declaring = frame.layer.source.identity.clone();
        frame.next += 1;
        if !is_local_parent(&written) {
            return Err(Error::InvalidParent {
                declaring,
                path: written,
            });
        }
        let source = match port.canonicalize(&declaring, &written) {
            Ok(source) => source,
            Err(SourceError::Unavailable) => {
                return Err(Error::Unavailable {
                    declaring,
                    path: written,
                })
            }
            Err(SourceError::Raised(error)) => return Err(Error::Host(error)),
        };
        if source.identity.is_empty() || source.display_path.is_empty() {
            return Err(Error::InvalidIdentity);
        }
        budget.text(&source.identity)?;
        budget.text(&source.display_path)?;
        budget.charge(
            Resource::Work,
            active
                .len()
                .saturating_add(completed.len())
                .saturating_mul(source.identity.len().saturating_add(1)),
        )?;
        if let Some(first) = active
            .iter()
            .position(|f| f.layer.source.identity == source.identity)
        {
            let mut path = Vec::new();
            for frame in &active[first..] {
                budget.text(&frame.layer.source.identity)?;
                path.push(frame.layer.source.identity.clone());
            }
            let returns_to_entry = source.identity == entry_identity;
            path.push(source.identity);
            return Err(Error::Cycle {
                path,
                returns_to_entry,
            });
        }
        if completed
            .iter()
            .any(|layer| layer.source.identity == source.identity)
        {
            continue;
        }
        budget.depth(active.len().saturating_add(1))?;
        budget.charge(Resource::Layers, 1)?;
        let input = match port.read(&source) {
            Ok(input) => input,
            Err(SourceError::Unavailable) => {
                return Err(Error::Unavailable {
                    declaring,
                    path: source.display_path,
                })
            }
            Err(SourceError::Raised(error)) => return Err(Error::Host(error)),
        };
        active.push(admit(
            schema,
            &entry_identity,
            source,
            input,
            normalization,
            budget,
        )?);
    }
    Ok(completed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhausted_counter_never_wraps_or_saturates_into_success() {
        let mut budget = Budget::new(Limits {
            work: usize::MAX,
            ..Limits::default()
        });
        budget.charge(Resource::Work, usize::MAX).unwrap();
        assert_eq!(
            budget.charge(Resource::Work, 1),
            Err(Exhausted {
                resource: Resource::Work,
                limit: usize::MAX
            })
        );
        assert!(budget.charge(Resource::Work, 1).is_err());
    }
}
