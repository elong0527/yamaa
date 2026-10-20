//! Borrowed environment reports admitted before the first owned diagnostic fact.
//!
//! The caller supplies the exact retained failed draft, lowered project root
//! and captured identities from one attempt. Matching identity strings or
//! equal contents cannot establish that ownership. Origin checks validate the
//! supplied retained relationships against the project root's declarations;
//! they do not manufacture provenance or authorize source reads. The project
//! file declares codelists and environment links; the linked environment file
//! declares the lock and functions, so each finding names the file that
//! declares its subject.
use super::{finding_diagnostic, scalar_diagnostic, Error as CoreError, Project};
use crate::json::Value as JsonValue;
use crate::{
    ast::{self, CodelistSource, FunctionSource},
    diagnostic::{ConditionCode, Diagnostic},
    project_environment::{Draft, Finding},
    project_function::Finding as FunctionFinding,
    project_function_document::Finding as ScalarFinding,
    value::Value,
};
use std::fmt::Write;
use std::vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("project environment diagnostics projection core: {0:?}")]
    Core(CoreError),
    #[error("project environment diagnostics projection invalid origin")]
    InvalidOrigin,
}
impl From<CoreError> for Error {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Origin<'a> {
    /// The environment file named by the project `environments` key.
    Environment(&'a str),
    Function(&'a str),
    Codelist(usize),
}
/// One captured file and the declaration that names it.
#[derive(Clone, Copy)]
pub struct Capture<'a> {
    pub origin: Origin<'a>,
    pub identity: &'a str,
}
/// Origin slices and captures are borrowed from the same retained attempt as
/// `draft` and `project`. Admission retains borrowed descriptors in a
/// collection, so an arbitrary input iterator is never replayed.
pub struct Admission<'a, I> {
    /// The project root, lowered with the environment file it links for the
    /// host when that file lowered.
    pub project: &'a ast::Project,
    /// The project file's identity.
    pub root: &'a str,
    /// The identity of the environment file the project links for the host,
    /// when captured.
    pub environment: Option<&'a str>,
    pub draft: &'a Draft,
    pub function_origins: &'a [String],
    pub codelist_origins: &'a [usize],
    pub captures: I,
}
#[derive(Clone, Copy)]
pub struct ScalarSource<'a> {
    pub capture: Capture<'a>,
    pub findings: &'a [ScalarFinding],
}
/// An immutable authority-bearing report. No owned fact exists before admission.
pub struct AdmissionReport<'a> {
    project: &'a ast::Project,
    root: &'a str,
    environment: Option<&'a str>,
    draft: &'a Draft,
    function_origins: &'a [String],
    codelist_origins: &'a [usize],
    captures: Vec<Capture<'a>>,
    findings: &'a [Finding],
}
pub struct RootScalarReport<'a> {
    root: &'a str,
    findings: &'a [ScalarFinding],
}
pub struct ScalarSourcesReport<'a> {
    root: &'a str,
    sources: Vec<ScalarSource<'a>>,
}

#[derive(Clone, Copy)]
enum Piece<'a> {
    Text(&'a str),
    Index(usize),
}
#[derive(Clone, Copy)]
struct Path<'a> {
    pieces: [Option<Piece<'a>>; 16],
    len: usize,
}
impl<'a> Path<'a> {
    fn new() -> Self {
        Self {
            pieces: [None; 16],
            len: 0,
        }
    }
    fn push(&mut self, piece: Piece<'a>) -> Result<(), CoreError> {
        let slot = self
            .pieces
            .get_mut(self.len)
            .ok_or(CoreError::UnavailableShape)?;
        *slot = Some(piece);
        self.len += 1;
        Ok(())
    }
    fn text(&mut self, text: &'a str) -> Result<(), CoreError> {
        self.push(Piece::Text(text))
    }
    fn index(&mut self, index: usize) -> Result<(), CoreError> {
        self.push(Piece::Index(index))
    }
    fn render(&self) -> String {
        let mut text = String::new();
        for piece in self.pieces[..self.len].iter().flatten() {
            match piece {
                Piece::Text(value) => text.push_str(value),
                Piece::Index(index) => {
                    write!(&mut text, "[{index}]").expect("writing to a String");
                }
            }
        }
        text
    }
    fn tail(&self, count: usize) -> Self {
        let mut path = Self::new();
        for piece in self.pieces[count..self.len].iter().flatten() {
            path.pieces[path.len] = Some(*piece);
            path.len += 1;
        }
        path
    }
}
// Provenance declarations have at most two pieces. Keeping their own fixed
// representation avoids a large context union or any pre-admission allocation.
#[derive(Clone, Copy)]
struct ContextPath<'a> {
    pieces: [Option<Piece<'a>>; 2],
    len: usize,
}
impl<'a> ContextPath<'a> {
    fn new(path: Path<'a>) -> Result<Self, CoreError> {
        if path.len > 2 {
            return Err(CoreError::UnavailableShape);
        }
        Ok(Self {
            pieces: [path.pieces[0], path.pieces[1]],
            len: path.len,
        })
    }
    fn render(self) -> String {
        let mut path = Path::new();
        path.pieces[..self.len].copy_from_slice(&self.pieces[..self.len]);
        path.len = self.len;
        path.render()
    }
}
#[derive(Clone, Copy)]
enum FieldValue<'a> {
    Text(&'a str),
    Scalar(&'a Value),
    Integer(&'a str),
    Path(ContextPath<'a>),
}
#[derive(Clone, Copy)]
struct Field<'a> {
    name: &'static str,
    value: FieldValue<'a>,
}
struct Record<'a> {
    code: ConditionCode,
    path: Path<'a>,
    fields: [Option<Field<'a>>; 8],
    len: usize,
}
impl<'a> Record<'a> {
    fn field(&mut self, name: &'static str, value: FieldValue<'a>) -> Result<(), CoreError> {
        if self.fields[..self.len]
            .iter()
            .flatten()
            .any(|field| field.name == name)
        {
            return Err(CoreError::UnavailableShape);
        }
        let slot = self
            .fields
            .get_mut(self.len)
            .ok_or(CoreError::UnavailableShape)?;
        *slot = Some(Field { name, value });
        self.len += 1;
        Ok(())
    }
    fn provenance(
        &mut self,
        source: &'a str,
        entry: &'a str,
        declaration: Path<'a>,
    ) -> Result<(), CoreError> {
        self.field("source", FieldValue::Text(source))?;
        self.field("entry", FieldValue::Text(entry))?;
        self.field(
            "environment_path",
            FieldValue::Path(ContextPath::new(declaration)?),
        )
    }
    fn materialize(self) -> Diagnostic {
        let context = self.fields[..self.len]
            .iter()
            .flatten()
            .map(|field| {
                let value = match field.value {
                    FieldValue::Text(text) => JsonValue::String(text.into()),
                    FieldValue::Scalar(value) => JsonValue::from(value.clone()),
                    FieldValue::Integer(text) => {
                        JsonValue::Number((text).parse().expect("canonical integer"))
                    }
                    FieldValue::Path(path) => JsonValue::String(path.render()),
                };
                (field.name.into(), value)
            })
            .collect();
        Diagnostic {
            code: self.code,
            spec_paths: vec![self.path.render()],
            context,
            source_span: None,
            operand_route: None,
        }
    }
}
struct Borrowed;
impl<'a> Project<'a> for Borrowed {
    type Output = Record<'a>;
    fn start(&mut self, code: ConditionCode, path: &'a str) -> Result<Record<'a>, CoreError> {
        let mut record = Record {
            code,
            path: Path::new(),
            fields: [None; 8],
            len: 0,
        };
        record.path.text(path)?;
        Ok(record)
    }
    fn append(&mut self, d: &mut Record<'a>, text: &'a str) -> Result<(), CoreError> {
        d.path.text(text)
    }
    fn index(&mut self, d: &mut Record<'a>, index: usize) -> Result<(), CoreError> {
        d.path.index(index)
    }
    fn context(
        &mut self,
        d: &mut Record<'a>,
        key: &'static str,
        text: &'a str,
    ) -> Result<(), CoreError> {
        d.field(key, FieldValue::Text(text))
    }
    fn scalar(
        &mut self,
        d: &mut Record<'a>,
        key: &'static str,
        value: &'a Value,
    ) -> Result<(), CoreError> {
        d.field(key, FieldValue::Scalar(value))
    }
    fn integer(
        &mut self,
        d: &mut Record<'a>,
        key: &'static str,
        text: &'a str,
    ) -> Result<(), CoreError> {
        d.field(key, FieldValue::Integer(text))
    }
}
/// The function declaration `name` keys, among those the environment file
/// declares.
fn function_declaration<'a>(
    project: &'a ast::Project,
    name: &str,
) -> Result<&'a ast::FunctionDeclaration, Error> {
    project
        .functions
        .as_deref()
        .ok_or(Error::InvalidOrigin)?
        .iter()
        .find(|declaration| declaration.key.as_ref().is_some_and(|key| key.name == name))
        .ok_or(Error::InvalidOrigin)
}
/// Whether the declaration `origin` names is written as a path, so names a
/// file of its own. Functions are declared by the linked environment file;
/// codelists and the environment link itself by the project file.
fn declares_file(project: &ast::Project, origin: Origin<'_>) -> Result<bool, Error> {
    match origin {
        Origin::Environment(key) => project
            .environments
            .iter()
            .find(|environment| environment.language.key() == key)
            .map(|environment| environment.file.is_some())
            .ok_or(Error::InvalidOrigin),
        Origin::Function(name) => Ok(matches!(
            function_declaration(project, name)?.source,
            FunctionSource::File(_)
        )),
        Origin::Codelist(index) => project
            .codelists
            .get(index)
            .map(|declaration| matches!(declaration.source, CodelistSource::File(_)))
            .ok_or(Error::InvalidOrigin),
    }
}
fn declaration(origin: Origin<'_>) -> Result<Path<'_>, CoreError> {
    let mut path = Path::new();
    match origin {
        Origin::Environment(key) => {
            path.text("environments.")?;
            path.text(key)?;
        }
        Origin::Function(name) => {
            path.text("functions.")?;
            path.text(name)?;
        }
        Origin::Codelist(index) => {
            path.text("codelists")?;
            path.index(index)?;
        }
    }
    Ok(path)
}
fn capture_for<'a, I: Iterator<Item = Capture<'a>> + Clone>(
    captures: &I,
    origin: Origin<'a>,
) -> Option<Capture<'a>> {
    captures.clone().find(|capture| capture.origin == origin)
}
fn validate_admission<'a, I: Iterator<Item = Capture<'a>> + Clone>(
    input: &Admission<'a, I>,
) -> Result<(), Error> {
    let functions = input.draft.functions.as_deref().unwrap_or(&[]);
    if input.function_origins.len() != functions.len()
        || input.codelist_origins.len() != input.draft.codelists.len()
        || input
            .function_origins
            .iter()
            .zip(functions)
            .any(|(name, function)| *name != function.name.name)
    {
        return Err(Error::InvalidOrigin);
    }
    for (index, capture) in input.captures.clone().enumerate() {
        if input
            .captures
            .clone()
            .take(index)
            .any(|earlier| earlier.origin == capture.origin)
        {
            return Err(Error::InvalidOrigin);
        }
        if !declares_file(input.project, capture.origin)? {
            return Err(Error::InvalidOrigin);
        }
        let included = match capture.origin {
            Origin::Function(name) => input.function_origins.iter().any(|origin| origin == name),
            Origin::Codelist(index) => input.codelist_origins.contains(&index),
            // The linked environment file is held separately from captures.
            Origin::Environment(_) => false,
        };
        if !included {
            return Err(Error::InvalidOrigin);
        }
    }
    let mut authored = match &input.project.functions {
        Some(declarations) => declarations.iter(),
        None if input.function_origins.is_empty() => [].iter(),
        None => return Err(Error::InvalidOrigin),
    };
    for name in input.function_origins {
        // Successful typed declarations are an ordered subsequence of authored
        // declarations. Failed captures leave holes, never reordered identities.
        let declaration = authored
            .by_ref()
            .find(|declaration| {
                declaration
                    .key
                    .as_ref()
                    .is_some_and(|key| key.name == *name)
            })
            .ok_or(Error::InvalidOrigin)?;
        let external = capture_for(&input.captures, Origin::Function(name)).is_some();
        if external != matches!(declaration.source, FunctionSource::File(_)) {
            return Err(Error::InvalidOrigin);
        }
    }
    for (typed, &index) in input.codelist_origins.iter().enumerate() {
        if typed > 0 && input.codelist_origins[typed - 1] >= index {
            return Err(Error::InvalidOrigin);
        }
        let external = capture_for(&input.captures, Origin::Codelist(index)).is_some();
        if external != declares_file(input.project, Origin::Codelist(index))? {
            return Err(Error::InvalidOrigin);
        }
    }
    Ok(())
}
fn admission_record<'a, I: Iterator<Item = Capture<'a>> + Clone>(
    input: &Admission<'a, I>,
    finding: &'a Finding,
) -> Result<Record<'a>, Error> {
    let mut record = finding_diagnostic(input.draft, finding, &mut Borrowed)?;
    let raw_path = record.path;
    let environment = || input.environment.ok_or(Error::InvalidOrigin);
    let (source, declaration) = match finding {
        Finding::Function { function, finding } => {
            let name = input
                .function_origins
                .get(*function)
                .ok_or(Error::InvalidOrigin)?;
            let origin = Origin::Function(name);
            let external = if matches!(finding, FunctionFinding::InvalidName) {
                None
            } else {
                capture_for(&input.captures, origin)
            };
            if external.is_some() {
                record.path = raw_path.tail(2);
                // The authored `functions.<name>.` separator is omitted without
                // inspecting or reparsing a rendered diagnostic path.
                let Some(Piece::Text(first)) = record.path.pieces[0] else {
                    return Err(Error::Core(CoreError::UnavailableShape));
                };
                let Some(relative) = first.strip_prefix('.') else {
                    return Err(Error::Core(CoreError::UnavailableShape));
                };
                record.path.pieces[0] = Some(Piece::Text(relative));
            }
            (
                match external {
                    Some(capture) => capture.identity,
                    None => environment()?,
                },
                declaration(origin)?,
            )
        }
        Finding::Codelist(finding) => {
            let index = *input
                .codelist_origins
                .get(finding.source)
                .ok_or(Error::InvalidOrigin)?;
            let origin = Origin::Codelist(index);
            let external = capture_for(&input.captures, origin);
            let suffix = raw_path.tail(2);
            if let Some(Piece::Text(first)) = suffix.pieces[0] {
                let Some(relative) = first.strip_prefix('.') else {
                    return Err(Error::Core(CoreError::UnavailableShape));
                };
                let mut relative_path = suffix;
                relative_path.pieces[0] = Some(Piece::Text(relative));
                record.path = if external.is_some() {
                    relative_path
                } else {
                    let mut path = declaration(origin)?;
                    path.text(".")?;
                    for piece in relative_path.pieces[..relative_path.len].iter().flatten() {
                        path.push(*piece)?;
                    }
                    path
                };
            } else {
                return Err(Error::Core(CoreError::UnavailableShape));
            }
            (
                external.map_or(input.root, |capture| capture.identity),
                declaration(origin)?,
            )
        }
        // The lock and duplicate function names are declared by the linked
        // environment file, at the field path the finding already names.
        Finding::LockKindMismatch { .. } | Finding::DuplicateFunction { .. } => {
            (environment()?, raw_path)
        }
    };
    record.provenance(source, input.root, declaration)?;
    Ok(record)
}
/// Consume the supplied capture iterator once, then admit the complete ordered
/// static report using immutable borrowed descriptors.
pub fn admit_admission<'a, I: Iterator<Item = Capture<'a>>>(
    input: Admission<'a, I>,
    findings: &'a [Finding],
) -> Result<AdmissionReport<'a>, Error> {
    let captures: Vec<_> = input.captures.collect();
    let input = Admission {
        project: input.project,
        root: input.root,
        environment: input.environment,
        draft: input.draft,
        function_origins: input.function_origins,
        codelist_origins: input.codelist_origins,
        captures: captures.iter().copied(),
    };
    validate_admission(&input)?;
    for finding in findings {
        admission_record(&input, finding)?;
    }
    Ok(AdmissionReport {
        project: input.project,
        root: input.root,
        environment: input.environment,
        draft: input.draft,
        function_origins: input.function_origins,
        codelist_origins: input.codelist_origins,
        captures,
        findings,
    })
}
impl AdmissionReport<'_> {
    pub fn diagnostics(&self) -> Result<Vec<Diagnostic>, Error> {
        let input = Admission {
            project: self.project,
            root: self.root,
            environment: self.environment,
            draft: self.draft,
            function_origins: self.function_origins,
            codelist_origins: self.codelist_origins,
            captures: self.captures.iter().copied(),
        };
        self.findings
            .iter()
            .map(|finding| admission_record(&input, finding).map(Record::materialize))
            .collect()
    }
}

fn root_record(finding: &ScalarFinding) -> Result<Record<'_>, Error> {
    scalar_diagnostic(finding, &mut Borrowed).map_err(Error::Core)
}
/// Failed inline project scalars report against the project root itself.
pub fn admit_root_scalars<'a>(
    root: &'a str,
    findings: &'a [ScalarFinding],
) -> Result<RootScalarReport<'a>, Error> {
    let mut declaration = Path::new();
    declaration.text("$")?;
    for finding in findings {
        root_record(finding)?.provenance(root, root, declaration)?;
    }
    Ok(RootScalarReport { root, findings })
}
impl RootScalarReport<'_> {
    pub fn diagnostics(&self) -> Result<Vec<Diagnostic>, Error> {
        let mut declaration = Path::new();
        declaration.text("$")?;
        let mut diagnostics = Vec::new();
        for finding in self.findings {
            let mut record = root_record(finding)?;
            record.provenance(self.root, self.root, declaration)?;
            diagnostics.push(record.materialize());
        }
        Ok(diagnostics)
    }
}
/// Consume source descriptors once. Semantic and opaque capture aggregation is
/// unchanged: this report includes scalar failures only, never replacing other
/// original failed captures or deciding their later boundary projection. A
/// failed function file is declared by the captured environment file; a failed
/// environment file or codelist file is declared by the project file.
pub fn admit_scalar_sources<'a, I: Iterator<Item = ScalarSource<'a>>>(
    project: &ast::Project,
    root: &'a str,
    sources: I,
) -> Result<ScalarSourcesReport<'a>, Error> {
    let sources: Vec<_> = sources.collect();
    for (index, source) in sources.iter().enumerate() {
        if sources[..index]
            .iter()
            .any(|earlier| earlier.capture.origin == source.capture.origin)
            || !declares_file(project, source.capture.origin)?
        {
            return Err(Error::InvalidOrigin);
        }
        let declaration = declaration(source.capture.origin)?;
        for finding in source.findings {
            scalar_diagnostic(finding, &mut Borrowed)?.provenance(
                source.capture.identity,
                root,
                declaration,
            )?;
        }
    }
    Ok(ScalarSourcesReport { root, sources })
}
impl ScalarSourcesReport<'_> {
    pub fn diagnostics(&self) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();
        for source in self.sources.iter() {
            let declaration = declaration(source.capture.origin)?;
            for finding in source.findings {
                let mut record = scalar_diagnostic(finding, &mut Borrowed)?;
                record.provenance(source.capture.identity, self.root, declaration)?;
                diagnostics.push(record.materialize());
            }
        }
        Ok(diagnostics)
    }
}
