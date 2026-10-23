//! Issue locations (P-ISSUE-1): the YAML file, line and column where an
//! issue's first spec path is written.
//!
//! A [`Locator`] holds the documents one operation read, each a root a path
//! can start in. A specification is held as the layers that wrote it, each
//! file as written, with its resolved document; a project file, the
//! environment, codelist and function files it reads, and the files a refusal
//! held are each held as written.
//!
//! [`Locator::locate`] takes an issue's first spec path, in either notation,
//! and walks it through each layer's normalized contribution: a
//! mapping member by its key, a list item by its index or by the `id` or
//! `name` it declares. A path into a shorthand expansion or composition
//! has the position of the deepest written node above:
//! the shorthand, the named window, the file that holds a dictionary.
//! Shorthand and long form reach the same contribution depth, so a later
//! shorthand can override a parent's structured value. Schema defaults have
//! no written position and stop the walk. Among a specification's layers the
//! deepest contribution wins and, on a tie, the latest layer, whose value took
//! effect (LAYER-3). Surviving composition provenance excludes earlier layers
//! hidden by whole-value replacement; a removed field locates at the clear
//! entry that removed it, unless the layer writes its container again.
//! An indexed row or intermediate (`rows[1]`) is the item
//! of the resolved specification at that position, found in each layer by
//! its `id`. Among roots the deepest node wins too; on a tie, the root the
//! issue's `declaring_source` names, the layer that writes the `parent` or
//! `include` entry naming a refused layer, then the root its `source` names,
//! and then the root held first. A project path that
//! reaches a file the project lists continues in that file's root
//! (`sdtm.specs[0].columns.SEX.label`). The location is the key of a mapping
//! member or the first character of a list item, one-based. A path that names
//! no written node is located only when it names a missing field of a root
//! whose class declares the field, at that root.
//!
//! A file the YAML reader refused has no nodes: `invalid_yaml` and
//! `non_ascii_source` keep the position the reader reported, in that file.
//! Positions never point into a data file: data files hold no spec path.

use yamaa_core::diagnostic::{ConditionCode, Diagnostic};
use yamaa_core::json::Value;
use yamaa_core::schema::{
    scalar_diagnostic_label, Document, DocumentNode as N, NormalizedDocument, SchemaSource,
};

use crate::specification_source::{CapturedDocument, PreparedDocument};
use crate::yaml_decode::{decode_yaml, DecodeFailure, DecodedYaml, SourcePosition};

/// Where an issue's first spec path is written: the file's identity, its
/// absolute path with forward slashes for a local file, and a one-based line
/// and column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub file: String,
    pub line: usize,
    pub column: usize,
}

/// A position in one of the locator's files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Written {
    file: usize,
    line: usize,
    column: usize,
}

/// One file as written, its positions, and its normalized contribution.
#[derive(Clone, Debug)]
struct Layer {
    file: usize,
    order: usize,
    document: Document,
    locations: Vec<SourcePosition>,
    contribution: Option<NormalizedDocument>,
}

impl Layer {
    fn at(&self, node: usize) -> Option<Written> {
        self.locations.get(node).map(|at| Written {
            file: self.file,
            line: at.line,
            column: at.column,
        })
    }

    /// How far this layer contributes along a path, including shorthand
    /// expansion. Defaults contribute nothing written by this layer.
    fn contribution_depth(&self, steps: &[Step]) -> usize {
        let Some(normalized) = &self.contribution else {
            return 0;
        };
        let document = &normalized.document;
        let mut node = document.root();
        let mut depth = 0;
        for step in steps {
            // The normalizer wraps an expression in an implicit `value`
            // member. Diagnostic paths omit that generated wrapper.
            while child(document, node, step).is_none() {
                let Some((key, value)) = child(document, node, &Step::Name("value".into())) else {
                    break;
                };
                if !normalized.origins[key].generated
                    || normalized.origins[key].source != SchemaSource::Input
                {
                    break;
                }
                node = value;
            }
            let Some((key, value)) = child(document, node, step) else {
                // A scalar replaces the inherited value whole. In 1.x an
                // expression such as RAW.NEW stays text until lowering; any
                // diagnostic path inside that expression belongs here.
                if !matches!(document.nodes()[node], N::Mapping(_) | N::Sequence(_)) {
                    return steps.len();
                }
                break;
            };
            if normalized.origins[key].source != SchemaSource::Input {
                break;
            }
            node = value;
            depth += 1;
        }
        depth
    }

    /// A 1.x clear entry that removed this path or a container above it.
    fn clearing(&self, path: &[Step]) -> Option<(usize, Written)> {
        let list = self.document.field(self.document.root(), "clear")?;
        let N::Sequence(items) = &self.document.nodes()[list] else {
            return None;
        };
        items
            .iter()
            .filter_map(|&item| {
                let N::Text(text) = &self.document.nodes()[item] else {
                    return None;
                };
                let cleared = steps(text);
                (!cleared.is_empty() && path.starts_with(&cleared))
                    .then(|| self.at(item).map(|at| (cleared.len(), at)))
                    .flatten()
            })
            .max_by_key(|&(depth, _)| depth)
    }
}

/// One document a path can start in.
#[derive(Clone, Debug)]
struct Root {
    /// The identity of the file the document was read from; for a
    /// specification, its entry file.
    identity: String,
    /// The files that wrote the document, the latest in contribution order
    /// first: a specification's layers, or one file.
    layers: Vec<Layer>,
    /// A specification's resolved document, whose rows and intermediates an
    /// indexed step selects.
    resolved: Option<Document>,
    /// Composition's surviving paths and their contributing layer. The
    /// nearest surviving ancestor bounds which layers can still contribute
    /// beneath a whole-value replacement, including a list or root field.
    provenance: Vec<(Vec<Step>, usize)>,
    /// The fields the document's class declares, when its schema is known.
    fields: Vec<String>,
    /// Text nodes of a file's document that name a file it lists, with that
    /// file's root.
    links: Vec<(usize, usize)>,
}

/// A file the YAML reader refused, and where.
#[derive(Clone, Debug)]
struct Refused {
    file: usize,
    non_ascii: bool,
    line: usize,
    column: usize,
}

/// The documents one operation read, which locate its issues (P-ISSUE-1).
#[derive(Clone, Debug, Default)]
pub struct Locator {
    files: Vec<String>,
    roots: Vec<Root>,
    refused: Vec<Refused>,
    /// Whether a specification's resolved document is held.
    specified: bool,
    /// Without one, the root of the file the operation was asked to read,
    /// held as written: a refused entry or project file, whose class is not
    /// known here.
    entry: Option<usize>,
}

/// One step of a spec path.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Step {
    Name(String),
    Index(usize),
}

/// The steps of a spec path written in either notation: `$` alone names the
/// root; a name, then `.name`, `[index]` or `['quoted']` steps. Parsing stops
/// at the first step outside the notation, keeping the steps before it.
fn steps(path: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    if path == "$" {
        return steps;
    }
    let bytes = path.as_bytes();
    let mut at = 0;
    loop {
        // A name runs to the next `.` or `[`.
        let end = bytes[at..]
            .iter()
            .position(|&byte| matches!(byte, b'.' | b'['))
            .map_or(bytes.len(), |offset| at + offset);
        if end > at {
            steps.push(Step::Name(path[at..end].to_owned()));
        } else if at == 0 || bytes.get(at.wrapping_sub(1)) == Some(&b'.') {
            // An empty name.
            return steps;
        }
        at = end;
        // Bracketed steps.
        while bytes.get(at) == Some(&b'[') {
            if bytes.get(at + 1) == Some(&b'\'') {
                let mut key = String::new();
                let mut index = at + 2;
                loop {
                    let Some(offset) = path[index..].find('\'') else {
                        return steps;
                    };
                    key.push_str(&path[index..index + offset]);
                    index += offset + 1;
                    if bytes.get(index) == Some(&b'\'') {
                        key.push('\'');
                        index += 1;
                    } else {
                        break;
                    }
                }
                if bytes.get(index) != Some(&b']') {
                    return steps;
                }
                steps.push(Step::Name(key));
                at = index + 1;
            } else {
                let Some(close) = path[at..].find(']').map(|offset| at + offset) else {
                    return steps;
                };
                let digits = &path[at + 1..close];
                if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                    return steps;
                }
                let Ok(index) = digits.parse() else {
                    return steps;
                };
                steps.push(Step::Index(index));
                at = close + 1;
            }
        }
        match bytes.get(at) {
            None => return steps,
            Some(b'.') => at += 1,
            Some(_) => return steps,
        }
    }
}

/// How far a path walked, and the deepest written node it reached.
#[derive(Clone, Copy)]
struct Walk {
    depth: usize,
    at: Option<Written>,
}

/// A scalar's text as a path writes it.
fn label(node: &N) -> Option<String> {
    match node {
        N::Text(text) => Some(text.clone()),
        node => scalar_diagnostic_label(node),
    }
}

/// The identity a list item declares: its `id`, or else its `name`.
fn identity(document: &Document, item: usize) -> Option<String> {
    ["id", "name"].iter().find_map(|field| {
        document
            .field(item, field)
            .and_then(|value| label(&document.nodes()[value]))
    })
}

/// The node `step` names inside `node` of `document`: the node whose
/// position it takes, a mapping member's key or the list item, and the node
/// a later step continues from.
fn child(document: &Document, node: usize, step: &Step) -> Option<(usize, usize)> {
    match (&document.nodes()[node], step) {
        (N::Mapping(entries), Step::Name(name)) => entries
            .iter()
            .find(|&&(key, _)| label(&document.nodes()[key]).as_deref() == Some(name))
            .copied(),
        (N::Sequence(items), Step::Index(index)) => items.get(*index).map(|&item| (item, item)),
        (N::Sequence(items), Step::Name(name)) => items
            .iter()
            .find(|&&item| identity(document, item).as_deref() == Some(name))
            .map(|&item| (item, item)),
        _ => None,
    }
}

impl Locator {
    /// A locator that holds no document: it locates no issue.
    pub fn none() -> Self {
        Self::default()
    }

    fn file(&mut self, identity: &str) -> usize {
        match self.files.iter().position(|file| file == identity) {
            Some(index) => index,
            None => {
                self.files.push(identity.to_owned());
                self.files.len() - 1
            }
        }
    }

    fn holds(&self, identity: &str) -> bool {
        self.files.iter().any(|file| file == identity)
    }

    fn layer(&mut self, identity: &str, decoded: &DecodedYaml) -> Layer {
        Layer {
            file: self.file(identity),
            order: 0,
            document: decoded.document.clone(),
            locations: decoded.locations.clone(),
            contribution: None,
        }
    }

    fn root(
        &mut self,
        identity: &str,
        layers: Vec<Layer>,
        resolved: Option<Document>,
        fields: Vec<String>,
    ) -> usize {
        self.roots.push(Root {
            identity: identity.to_owned(),
            layers,
            resolved,
            provenance: Vec::new(),
            fields,
            links: Vec::new(),
        });
        self.roots.len() - 1
    }

    /// A file read and admitted as written, such as a project file.
    fn captured(&mut self, document: &CapturedDocument) -> usize {
        let fields = document
            .schema()
            .structure()
            .class_named(document.class_name())
            .map(|class| {
                class
                    .fields
                    .iter()
                    .map(|field| field.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let identity = &document.source().identity;
        let layer = self.layer(identity, document.raw());
        self.root(identity, vec![layer], None, fields)
    }

    /// The specification `document` resolves to: each layer as written and
    /// normalized for composition, the latest first, and its resolved document.
    pub(crate) fn specification(&mut self, document: &PreparedDocument) -> usize {
        let entry = document.source().identity.as_str();
        let specification = document.specification();
        let resolved = specification.resolved();
        // The layers in contribution order, the root ancestor first.
        let contributed: Vec<(&str, Option<NormalizedDocument>)> = specification
            .layers()
            .iter()
            .map(|layer| {
                (
                    layer.source.identity.as_str(),
                    document
                        .schema()
                        .structure()
                        .at_version(layer.version)
                        .normalize_versioned_layer(&layer.document)
                        .ok(),
                )
            })
            .collect();
        let decoded = |identity: &str| -> Option<&DecodedYaml> {
            if identity == entry {
                return Some(document.raw());
            }
            document
                .parents()
                .iter()
                .find(|parent| parent.source().identity == identity)
                .map(|parent| parent.raw())
        };
        let mut layers = Vec::new();
        for (order, (identity, normalized)) in contributed.into_iter().enumerate().rev() {
            if let Some(raw) = decoded(identity) {
                let mut layer = self.layer(identity, raw);
                layer.order = order;
                layer.contribution = normalized;
                layers.push(layer);
            }
        }
        if layers.is_empty() {
            let layer = self.layer(entry, document.raw());
            layers.push(layer);
        }
        let fields = document
            .schema()
            .structure()
            .root_class()
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect();
        self.specified = true;
        let root = self.root(entry, layers, Some(resolved.clone()), fields);
        self.roots[root].provenance = specification
            .provenance()
            .iter()
            .map(|origin| (steps(&origin.path), origin.layer))
            .collect();
        root
    }

    /// A project file and the files it reads: each codelist and function
    /// file, each linked from the field that names it, and the lock, which
    /// holds no spec path. Returns the project file's root.
    fn environment(&mut self, files: ProjectFiles<'_>) -> Option<usize> {
        use crate::project_source::Origin;
        let project = files.root.map(|root| self.captured(root));
        if let Some(lock) = files.lock {
            self.file(lock);
        }
        for capture in files.captures {
            let target = self.captured(&capture.document);
            let path = match &capture.origin {
                Origin::FunctionFile(index) => format!("functions[{index}]"),
                Origin::Codelist(index) => format!("codelists[{index}]"),
                Origin::Lock => continue,
            };
            if let Some(from) = project {
                self.link(from, &path, target);
            }
        }
        project
    }

    /// Continue a path that reaches the text at `path` in the file `from`
    /// holds into `to`.
    fn link(&mut self, from: usize, path: &str, to: usize) {
        let Some(layer) = self.roots[from].layers.first() else {
            return;
        };
        let document = &layer.document;
        let mut node = document.root();
        for step in steps(path) {
            match child(document, node, &step) {
                Some((_, value)) => node = value,
                None => return,
            }
        }
        if matches!(document.nodes()[node], N::Text(_)) {
            self.roots[from].links.push((node, to));
        }
    }

    /// Every source a refusal held that no root holds: a YAML file kept as
    /// written, or a file the YAML reader refused, kept with the position it
    /// reported.
    pub(crate) fn sources(&mut self, sources: &crate::file_resources::RetainedSources) {
        for (identity, bytes) in sources {
            if self.holds(identity) {
                continue;
            }
            match decode_yaml(bytes) {
                Ok(decoded) => {
                    if matches!(
                        decoded.document.nodes()[decoded.document.root()],
                        N::Mapping(_)
                    ) {
                        let layer = self.layer(identity, &decoded);
                        let root = self.root(identity, vec![layer], None, Vec::new());
                        // Capture reads the file it was asked for first.
                        if !self.specified && self.entry.is_none() {
                            self.entry = Some(root);
                        }
                    }
                }
                Err(DecodeFailure::NonAscii(at)) => self.refuse(identity, true, at),
                Err(DecodeFailure::InvalidYaml { position, .. }) => {
                    self.refuse(identity, false, position)
                }
                Err(_) => {}
            }
        }
    }

    fn refuse(&mut self, identity: &str, non_ascii: bool, at: SourcePosition) {
        let file = self.file(identity);
        self.refused.push(Refused {
            file,
            non_ascii,
            line: at.line,
            column: at.column,
        });
    }

    /// `steps` as a specification's layers write them: a row or intermediate
    /// that a path names by its position in the resolved specification, by
    /// the `id` it declares.
    fn written_steps(&self, root: usize, steps: &[Step]) -> Vec<Step> {
        let mut written = steps.to_vec();
        let Some(resolved) = &self.roots[root].resolved else {
            return written;
        };
        if let [Step::Name(list), Step::Index(index), ..] = steps {
            if matches!(list.as_str(), "rows" | "intermediates") {
                let item = resolved
                    .field(resolved.root(), list)
                    .and_then(|items| match &resolved.nodes()[items] {
                        N::Sequence(items) => items.get(*index).copied(),
                        _ => None,
                    });
                if let Some(id) = item.and_then(|item| identity(resolved, item)) {
                    written[1] = Step::Name(id);
                }
            }
        }
        written
    }

    /// Walk `steps` through every layer of `root`, keeping the deepest
    /// contribution, and on a tie the latest layer's written position.
    fn walk(&self, root: usize, steps: &[Step], visited: &mut Vec<usize>) -> Walk {
        let written = self.written_steps(root, steps);
        let minimum = self.roots[root]
            .provenance
            .iter()
            .filter(|(path, _)| written.starts_with(path))
            .max_by_key(|(path, order)| (path.len(), *order))
            .map_or(0, |(_, order)| *order);
        let mut best: Option<Walk> = None;
        for layer in &self.roots[root].layers {
            if layer.order < minimum {
                continue;
            }
            let mut walk = self.walk_layer(root, layer, &written, visited);
            if let Some((depth, at)) = layer.clearing(&written) {
                if walk.depth < depth {
                    walk.at = Some(at);
                }
                walk.depth = written.len();
            }
            // Compare effective contributions, but keep the position of the
            // deepest node written in this layer. A shorthand reaches its
            // expanded path without having written those extra keys.
            walk.depth = walk.depth.max(layer.contribution_depth(&written));
            if best.is_none_or(|best| walk.depth > best.depth) {
                best = Some(walk);
            }
        }
        best.unwrap_or(Walk { depth: 0, at: None })
    }

    /// Walk `steps` through one layer of `root`, continuing into a listed
    /// file's root where a project path reaches the text that names it.
    fn walk_layer(
        &self,
        root: usize,
        layer: &Layer,
        steps: &[Step],
        visited: &mut Vec<usize>,
    ) -> Walk {
        let document = &layer.document;
        let mut node = document.root();
        let mut walk = Walk {
            depth: 0,
            at: layer.at(node),
        };
        while let Some(step) = steps.get(walk.depth) {
            if let Some((key, value)) = child(document, node, step) {
                walk.at = layer.at(key).or(walk.at);
                node = value;
                walk.depth += 1;
                continue;
            }
            let link = self.roots[root]
                .links
                .iter()
                .find(|(text, _)| *text == node)
                .map(|&(_, to)| to)
                .filter(|to| !visited.contains(to));
            if let Some(to) = link {
                visited.push(to);
                let rest = self.walk(to, &steps[walk.depth..], visited);
                walk.depth += rest.depth;
                walk.at = rest.at.or(walk.at);
            }
            break;
        }
        walk
    }

    /// Where `diagnostic`'s first spec path is written; see the module
    /// documentation. `None` when no root holds it.
    pub fn locate(&self, diagnostic: &Diagnostic) -> Option<Location> {
        if matches!(
            diagnostic.code,
            ConditionCode::SourceInvalidYaml | ConditionCode::SourceNonAscii
        ) {
            return self.refused_at(diagnostic);
        }
        let path = diagnostic.spec_paths.first()?;
        let steps = steps(path);
        let text = |key: &str| match diagnostic.context.get(key) {
            Some(Value::String(text)) => Some(text.as_str()),
            _ => None,
        };
        let source = text("source");
        // The layer that writes the field naming the file `source` names,
        // which the entry or another layer may write too.
        let declaring = text("declaring_source");
        // A missing field the issue names, which its path ends with.
        let missing = text("field");
        let mut order = (0..self.roots.len()).collect::<Vec<_>>();
        order.sort_by_key(|&root| {
            let identity = Some(self.roots[root].identity.as_str());
            (identity != declaring, identity != source)
        });
        let mut best: Option<(usize, Written)> = None;
        for (rank, &root) in order.iter().enumerate() {
            let walk = self.walk(root, &steps, &mut vec![root]);
            let eligible = walk.depth > 0
                || (steps.is_empty() && rank == 0)
                || matches!(steps.as_slice(), [Step::Name(field)]
                    if self.roots[root].fields.contains(field)
                        || (self.entry == Some(root) && missing == Some(field.as_str())));
            let Some(at) = walk.at.filter(|_| eligible) else {
                continue;
            };
            if best.is_none_or(|(depth, _)| walk.depth > depth) {
                best = Some((walk.depth, at));
            }
        }
        best.map(|(_, at)| self.location(at))
    }

    /// A refused file's issue, at the position the reader reported: the one
    /// refused file that reported it, or the one the issue names.
    fn refused_at(&self, diagnostic: &Diagnostic) -> Option<Location> {
        let number = |key: &str| match diagnostic.context.get(key) {
            Some(Value::Number(number)) => number.as_u64().and_then(|n| usize::try_from(n).ok()),
            _ => None,
        };
        let (line, column) = (number("line")?, number("column")?);
        let non_ascii = diagnostic.code == ConditionCode::SourceNonAscii;
        let matching = self
            .refused
            .iter()
            .filter(|refused| {
                refused.non_ascii == non_ascii && (refused.line, refused.column) == (line, column)
            })
            .collect::<Vec<_>>();
        let refused = match matching.as_slice() {
            [one] => *one,
            many => {
                let named = match diagnostic.context.get("path") {
                    Some(Value::String(path)) => path.as_str(),
                    _ => return None,
                };
                many.iter()
                    .find(|refused| self.files[refused.file] == named)?
            }
        };
        Some(self.location(Written {
            file: refused.file,
            line,
            column,
        }))
    }

    fn location(&self, at: Written) -> Location {
        Location {
            file: self.files[at.file].clone(),
            line: at.line,
            column: at.column,
        }
    }
}

/// The locator of a prepared specification: its layers, then the project
/// file and the files it reads, when it has a project.
pub fn prepared(prepared: &crate::file_preparation::FileSpecification) -> Locator {
    let mut locator = Locator::default();
    let run = prepared.run();
    locator.specification(run.document());
    locator.environment(run.captured_environment().into());
    locator
}

/// The locator of a refused selection: the specification as far as it
/// resolved, the project files read, and every other file the refusal held.
pub fn selection(error: &crate::file_workflow::SelectionError) -> Locator {
    use crate::file_workflow::SelectionError;
    match error {
        SelectionError::Compile(rejected) => {
            let run = rejected.run();
            let mut locator = Locator::default();
            locator.specification(&run.document);
            locator.environment((&run.captured).into());
            locator.sources(&rejected.retained_metadata_sources());
            locator
        }
        SelectionError::Preparation(error) => workflow(error),
    }
}

/// The locator of a refused capture of a project or an entry: the entry as
/// far as it resolved, the project files read, and every other file the
/// refusal held.
pub fn workflow(error: &crate::file_workflow::Error) -> Locator {
    use crate::file_workflow::Error;
    let mut locator = Locator::default();
    match error {
        Error::Entry(rejected) => {
            if let Some(document) = rejected.document() {
                locator.specification(document);
            }
            locator.environment(rejected.environment().into());
            locator.sources(&rejected.retained_metadata_sources());
        }
        Error::Environment(rejected) => locator.sources(&rejected.retained_metadata_sources()),
        Error::Caller { .. } | Error::Schema(_) => {}
    }
    locator
}

/// A project's locator: the project file and the files it reads, each
/// listed specification, linked from the listing that names it
/// (`sdtm.specs[0]`), and every other file it held.
pub(crate) fn project<'a>(
    files: ProjectFiles<'_>,
    specifications: impl IntoIterator<Item = (&'a str, &'a PreparedDocument)>,
    sources: &crate::file_resources::RetainedSources,
) -> Locator {
    let mut locator = Locator::default();
    let project = locator.environment(files);
    for (listing, document) in specifications {
        let target = locator.specification(document);
        if let Some(project) = project {
            locator.link(project, listing, target);
        }
    }
    locator.sources(sources);
    locator
}

/// The files a project reads, as an environment holds them: the project
/// file, the codelist and function files, and the lock.
#[derive(Clone, Copy, Default)]
pub(crate) struct ProjectFiles<'a> {
    pub(crate) root: Option<&'a CapturedDocument>,
    pub(crate) captures: &'a [crate::project_source::DocumentCapture],
    pub(crate) lock: Option<&'a str>,
}

impl<'a> From<&'a crate::project_source::CapturedEnvironment> for ProjectFiles<'a> {
    fn from(captured: &'a crate::project_source::CapturedEnvironment) -> Self {
        Self {
            root: captured.root_optional(),
            captures: captured.captures(),
            lock: captured.lock().map(|lock| lock.source.identity.as_str()),
        }
    }
}

impl<'a> From<&'a crate::project_source::OwnedEnvironment> for ProjectFiles<'a> {
    fn from(owned: &'a crate::project_source::OwnedEnvironment) -> Self {
        Self {
            root: owned.root_optional(),
            captures: owned.captures(),
            lock: owned.lock().map(|lock| lock.source.identity.as_str()),
        }
    }
}

#[cfg(all(test, feature = "yaml"))]
mod tests {
    use super::*;
    use std::sync::Arc;
    use yamaa_core::diagnostic::Context;

    fn issue(path: &str, context: Context) -> Diagnostic {
        Diagnostic {
            code: ConditionCode::ColumnForwardReference,
            spec_paths: vec![path.into()],
            context,
            source_span: None,
            operand_route: None,
        }
    }

    fn layer(locator: &mut Locator, identity: &str, text: &str) -> Layer {
        locator.layer(identity, &decode_yaml(text.as_bytes()).unwrap())
    }

    fn file(locator: &mut Locator, identity: &str, text: &str, fields: &[&str]) -> usize {
        let layer = layer(locator, identity, text);
        let fields = fields.iter().map(|field| (*field).to_owned()).collect();
        locator.root(identity, vec![layer], None, fields)
    }

    fn at(locator: &Locator, path: &str) -> Option<(String, usize, usize)> {
        locator
            .locate(&issue(path, Context::new()))
            .map(|location| (location.file, location.line, location.column))
    }

    #[test]
    fn paths_in_either_notation_split_into_steps() {
        // Each step written `name` or `[index]`, joined by `|`.
        let split = |path: &str| {
            steps(path)
                .iter()
                .map(|step| match step {
                    Step::Name(name) => name.clone(),
                    Step::Index(index) => format!("[{index}]"),
                })
                .collect::<Vec<_>>()
                .join("|")
        };
        assert_eq!(split("$"), "");
        assert_eq!(
            split("columns.AVAL.derivation.compute"),
            "columns|AVAL|derivation|compute"
        );
        assert_eq!(
            split("rows[1].derivations.WBCVAL"),
            "rows|[1]|derivations|WBCVAL"
        );
        assert_eq!(
            split("columns.RACEN.mapping.dict['O''BRIEN']"),
            "columns|RACEN|mapping|dict|O'BRIEN"
        );
        assert_eq!(
            split("columns.RACEN.mapping.dict.NOT REPORTED"),
            "columns|RACEN|mapping|dict|NOT REPORTED"
        );
        assert_eq!(
            split("codelists[0].codelists[0].format_name"),
            "codelists|[0]|codelists|[0]|format_name"
        );
        // Parsing keeps the steps before the first one outside the notation.
        assert_eq!(split("keys[x].y"), "keys");
        assert_eq!(split("a..b"), "a");
        assert_eq!(split(""), "");
    }

    #[test]
    fn a_path_locates_at_the_key_of_its_deepest_written_member() {
        let mut locator = Locator::default();
        file(
            &mut locator,
            "/study/spec.yaml",
            "domain: DM\ncolumns:\n  - name: AGE\n    derivation:\n      compute: {expr: AGE + 1}\n  - {name: SEX, derivation: DM.SEX}\nrows:\n  - id: first\n    filter: X\n",
            &["domain", "columns", "rows", "model"],
        );
        let file = || "/study/spec.yaml".to_owned();
        assert_eq!(
            at(&locator, "columns.AGE.derivation.compute"),
            Some((file(), 5, 7))
        );
        // A member by identity is located at its first character; a path
        // that goes past what was written stops at the deepest written node,
        // such as a shorthand.
        assert_eq!(at(&locator, "columns.AGE"), Some((file(), 3, 5)));
        assert_eq!(at(&locator, "columns.AGE.label"), Some((file(), 3, 5)));
        assert_eq!(
            at(&locator, "columns.SEX.derivation.source"),
            Some((file(), 6, 17))
        );
        assert_eq!(at(&locator, "rows[0].filter"), Some((file(), 9, 5)));
        assert_eq!(at(&locator, "rows.first.filter"), Some((file(), 9, 5)));
        // A missing field the root's class declares is located at the root;
        // a path that names nothing else of the document is not located.
        assert_eq!(at(&locator, "model"), Some((file(), 1, 1)));
        assert_eq!(at(&locator, "created"), None);
        assert_eq!(at(&locator, "datasets.DM.columns.AGE"), None);
        assert_eq!(at(&locator, "$"), Some((file(), 1, 1)));
    }

    #[test]
    fn the_latest_layer_that_wrote_the_deepest_node_wins() {
        let mut locator = Locator::default();
        let parent = layer(
            &mut locator,
            "/study/parent.yaml",
            "domain: DM\ncolumns:\n  - name: AGE\n    type: int\n    derivation:\n      compute: {expr: AGE + 1}\nrows:\n  - id: first\n  - id: second\n    filter: X\n",
        );
        let child = layer(
            &mut locator,
            "/study/child.yaml",
            "parent: parent.yaml\ncolumns:\n  - name: AGE\n    derivation:\n      compute: {expr: AGE + 2}\nrows:\n  - id: second\n    filter: Y\n",
        );
        // The resolved specification lists the rows in another order.
        let resolved = decode_yaml(b"rows:\n  - id: second\n  - id: first\n")
            .unwrap()
            .document;
        locator.root(
            "/study/child.yaml",
            vec![child, parent],
            Some(resolved),
            Vec::new(),
        );
        let parent = || "/study/parent.yaml".to_owned();
        let child = || "/study/child.yaml".to_owned();
        // Both layers write the derivation: the child's value took effect.
        assert_eq!(
            at(&locator, "columns.AGE.derivation.compute"),
            Some((child(), 5, 7))
        );
        // Only the parent writes the type.
        assert_eq!(at(&locator, "columns.AGE.type"), Some((parent(), 4, 5)));
        // `rows[0]` is the resolved specification's first row, `second`.
        assert_eq!(at(&locator, "rows[0].filter"), Some((child(), 8, 5)));
        assert_eq!(at(&locator, "rows[1]"), Some((parent(), 8, 5)));
    }

    #[test]
    fn a_project_path_continues_into_the_file_it_lists() {
        let mut locator = Locator::default();
        let project = file(
            &mut locator,
            "/study/project.yaml",
            "sdtm:\n  specs:\n    - dm.yaml\n",
            &[],
        );
        let spec = file(
            &mut locator,
            "/study/dm.yaml",
            "domain: DM\ncolumns:\n  - name: SEX\n    label: Sex\n",
            &[],
        );
        locator.link(project, "sdtm.specs[0]", spec);
        assert_eq!(
            at(&locator, "sdtm.specs[0].columns.SEX.label"),
            Some(("/study/dm.yaml".to_owned(), 4, 5))
        );
        assert_eq!(
            at(&locator, "sdtm.specs[0]"),
            Some(("/study/project.yaml".to_owned(), 3, 7))
        );
    }

    #[test]
    fn the_root_the_issue_names_wins_a_tie() {
        let mut locator = Locator::default();
        file(
            &mut locator,
            "/study/spec.yaml",
            "parent: base.yaml\ndomain: DM\n",
            &[],
        );
        file(
            &mut locator,
            "/study/base.yaml",
            "domain: DM\nparent: spec.yaml\n",
            &[],
        );
        // Both files write `parent`: without a name the file held first wins,
        // then the file `source` names, and over it the file that writes the
        // field naming that file, `declaring_source`.
        for (names, file, line, label) in [
            (&[][..], "/study/spec.yaml", 1, "no name"),
            (
                &[("source", "/study/base.yaml")][..],
                "/study/base.yaml",
                2,
                "source",
            ),
            (
                &[
                    ("source", "/study/spec.yaml"),
                    ("declaring_source", "/study/base.yaml"),
                ][..],
                "/study/base.yaml",
                2,
                "declaring_source over source",
            ),
        ] {
            let context = names
                .iter()
                .map(|&(key, value)| (key.to_owned(), Value::String(value.into())))
                .collect();
            assert_eq!(
                locator.locate(&issue("parent", context)),
                Some(Location {
                    file: file.into(),
                    line,
                    column: 1
                }),
                "{label}"
            );
        }
    }

    #[test]
    fn retained_inputs_are_not_decoded_or_kept_as_yaml_refusals() {
        let directory = std::env::temp_dir().join(format!(
            "yamaa-location-source-kinds-{}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let directory = crate::canonical_path::canonicalize(directory).unwrap();
        for (name, bytes) in [
            ("input.xpt", &b"\xff"[..]),
            ("input.csv", &b"domain: DATA\n"[..]),
            ("invalid.yaml", &b"\xff"[..]),
            ("spec.yaml", &b"domain: DM\n"[..]),
        ] {
            std::fs::write(directory.join(name), bytes).unwrap();
        }
        let root = directory.to_str().unwrap();
        let mut resources = crate::file_resources::Resources::new(root, root, &[]).unwrap();
        resources.capture("input.xpt").unwrap();
        resources.capture("input.csv").unwrap();
        resources.capture_metadata("invalid.yaml").unwrap();
        resources.capture_metadata("spec.yaml").unwrap();
        // Complete evidence still owns both inputs, including the binary.
        assert_eq!(resources.retain_sources().len(), 4);
        let mut locator = Locator::none();
        locator.sources(&resources.retain_metadata_sources());
        assert_eq!(locator.files.len(), 2);
        assert_eq!(locator.roots.len(), 1);
        assert_eq!(locator.refused.len(), 1);
        assert!(locator.files.iter().all(|file| file.ends_with(".yaml")));
        let path = directory
            .join("spec.yaml")
            .to_str()
            .unwrap()
            .replace('\\', "/");
        assert_eq!(at(&locator, "domain"), Some((path, 1, 1)));
        assert_eq!(locator.refused[0].line, 1);
        assert_eq!(locator.refused[0].column, 1);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_refused_file_keeps_the_reader_position() {
        let mut locator = Locator::default();
        let sources = vec![
            (
                "/study/spec.yaml".to_owned(),
                Arc::from(&b"domain: [DM\n"[..]),
            ),
            (
                "/study/good.yaml".to_owned(),
                Arc::from(&b"domain: DM\n"[..]),
            ),
        ];
        locator.sources(&sources);
        let DecodeFailure::InvalidYaml { position, .. } =
            decode_yaml(b"domain: [DM\n").unwrap_err()
        else {
            panic!("invalid YAML");
        };
        let mut invalid = issue(
            "$",
            [
                ("line".to_owned(), Value::from(position.line)),
                ("column".to_owned(), Value::from(position.column)),
            ]
            .into(),
        );
        invalid.code = ConditionCode::SourceInvalidYaml;
        assert_eq!(
            locator.locate(&invalid),
            Some(Location {
                file: "/study/spec.yaml".into(),
                line: position.line,
                column: position.column
            })
        );
        // Another position names no refused file.
        invalid.context.insert("line".into(), Value::from(99_u64));
        assert_eq!(locator.locate(&invalid), None);
        // The file read first, the refused entry's own, locates a missing
        // field the issue names at its root.
        let mut missing = issue(
            "model",
            [("field".to_owned(), Value::String("model".into()))].into(),
        );
        missing.code = ConditionCode::SchemaMissingRequiredField;
        assert_eq!(
            locator.locate(&missing),
            Some(Location {
                file: "/study/good.yaml".into(),
                line: 1,
                column: 1
            })
        );
    }
}
