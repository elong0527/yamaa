//! Borrowed environment reports admitted before the first owned diagnostic fact.
//!
//! The caller supplies the exact retained failed draft, lowered project root
//! and captured identities from one attempt. Matching identity strings or
//! equal contents cannot establish that ownership. Origin checks validate the
//! supplied retained relationships against the project root's declarations;
//! they do not manufacture provenance or authorize source reads. A project
//! file declares codelists, the lock and the function files, and each
//! function file declares its own definition (D8). Each finding names the
//! file that declares its subject.
use super::{finding_diagnostic, scalar_diagnostic, Error as CoreError, Project};
use crate::json::Value as JsonValue;
use crate::{
    ast::{self, CodelistSource},
    diagnostic::{ConditionCode, Diagnostic},
    project_environment::{Draft, Finding},
    project_function_document::{Finding as ScalarFinding, Kind as ScalarKind},
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
pub enum Origin {
    /// A function file the project lists, by its index in `functions`.
    FunctionFile(usize),
    Codelist(usize),
}
/// One captured file and the declaration that names it.
#[derive(Clone, Copy)]
pub struct Capture<'a> {
    pub origin: Origin,
    pub identity: &'a str,
}
/// Origin slices and captures are borrowed from the same retained attempt as
/// `draft` and `project`. Admission retains borrowed descriptors in a
/// collection, so an arbitrary input iterator is never replayed.
pub struct Admission<'a, I> {
    /// The project root.
    pub project: &'a ast::Project,
    /// The project file's identity.
    pub root: &'a str,
    pub draft: &'a Draft,
    pub function_origins: &'a [String],
    /// The index in `functions` of each definition's file.
    pub function_files: &'a [usize],
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
    draft: &'a Draft,
    function_origins: &'a [String],
    function_files: &'a [usize],
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
/// Whether the declaration `origin` names is written as a path, so names a
/// file of its own: every listed function is a function file, and a codelist
/// source is a file or written in place.
fn declares_file(project: &ast::Project, origin: Origin) -> Result<bool, Error> {
    match origin {
        Origin::FunctionFile(index) => project
            .functions
            .as_deref()
            .and_then(|declarations| declarations.get(index))
            .map(|_| true)
            .ok_or(Error::InvalidOrigin),
        Origin::Codelist(index) => project
            .codelists
            .get(index)
            .map(|declaration| matches!(declaration.source, CodelistSource::File(_)))
            .ok_or(Error::InvalidOrigin),
    }
}
fn declaration<'a>(origin: Origin) -> Result<Path<'a>, CoreError> {
    let mut path = Path::new();
    match origin {
        Origin::FunctionFile(index) => {
            path.text("functions")?;
            path.index(index)?;
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
    origin: Origin,
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
        || input.function_files.len() != functions.len()
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
            Origin::FunctionFile(index) => input.function_files.contains(&index),
            Origin::Codelist(index) => input.codelist_origins.contains(&index),
        };
        if !included {
            return Err(Error::InvalidOrigin);
        }
    }
    // Every definition is a listed function file, held in listing order;
    // failed captures leave holes, never reordered files.
    for (typed, &index) in input.function_files.iter().enumerate() {
        if (typed > 0 && input.function_files[typed - 1] >= index)
            || capture_for(&input.captures, Origin::FunctionFile(index)).is_none()
        {
            return Err(Error::InvalidOrigin);
        }
    }
    codelist_order(input)
}
/// Typed codelist sources are an ordered subsequence of the declared ones, a
/// held capture exactly for each declared file.
fn codelist_order<'a, I: Iterator<Item = Capture<'a>> + Clone>(
    input: &Admission<'a, I>,
) -> Result<(), Error> {
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
    // A function file declares its own definition, whose finding already
    // names its path in that file.
    let function_file = |function: usize| -> Result<(&'a str, Path<'a>), Error> {
        let index = *input
            .function_files
            .get(function)
            .ok_or(Error::InvalidOrigin)?;
        let origin = Origin::FunctionFile(index);
        let capture = capture_for(&input.captures, origin).ok_or(Error::InvalidOrigin)?;
        Ok((capture.identity, declaration(origin)?))
    };
    let (source, declaration) = match finding {
        Finding::Function { function, .. } | Finding::DuplicateFunction { function } => {
            function_file(*function)?
        }
        // The lock is declared by the project file, at the field the finding
        // already names.
        Finding::LockKindMismatch { .. } => (input.root, raw_path),
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
        draft: input.draft,
        function_origins: input.function_origins,
        function_files: input.function_files,
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
        draft: input.draft,
        function_origins: input.function_origins,
        function_files: input.function_files,
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
            draft: self.draft,
            function_origins: self.function_origins,
            function_files: self.function_files,
            codelist_origins: self.codelist_origins,
            captures: self.captures.iter().copied(),
        };
        self.findings
            .iter()
            .map(|finding| admission_record(&input, finding).map(Record::materialize))
            .collect()
    }
}

/// The least and greatest `int`, which a literal outside the range reports.
const INT_MIN: &str = "-9223372036854775808";
const INT_MAX: &str = "9223372036854775807";

/// A scalar finding's record in the file `origin` names, or the project file
/// itself. A function file's fails as its file does (FUNC-2), and a codelist
/// source's integer outside the `int` range fails as any runtime literal
/// does, with `integer_overflow` (VALUE-5); a codelist source holds no other
/// scalar the engine cannot read.
fn scalar_record(finding: &ScalarFinding, origin: Option<Origin>) -> Result<Record<'_>, Error> {
    match origin {
        Some(Origin::FunctionFile(_)) => {
            scalar_diagnostic(finding, ConditionCode::FunctionFileInvalid, &mut Borrowed)
                .map_err(Error::Core)
        }
        Some(Origin::Codelist(_)) | None => {
            let (ScalarKind::IntegerRange, Some(value)) = (finding.kind, finding.value.as_deref())
            else {
                return Err(Error::Core(CoreError::UnavailableShape));
            };
            let mut record =
                Borrowed.start(ConditionCode::LiteralIntegerOverflow, &finding.path)?;
            record.field("actual", FieldValue::Integer(value))?;
            record.field("minimum", FieldValue::Integer(INT_MIN))?;
            record.field("maximum", FieldValue::Integer(INT_MAX))?;
            Ok(record)
        }
    }
}
/// Failed inline scalars of a project file report against the project root
/// itself.
pub fn admit_root_scalars<'a>(
    root: &'a str,
    findings: &'a [ScalarFinding],
) -> Result<RootScalarReport<'a>, Error> {
    let mut declaration = Path::new();
    declaration.text("$")?;
    for finding in findings {
        scalar_record(finding, None)?.provenance(root, root, declaration)?;
    }
    Ok(RootScalarReport { root, findings })
}
impl RootScalarReport<'_> {
    pub fn diagnostics(&self) -> Result<Vec<Diagnostic>, Error> {
        let mut declaration = Path::new();
        declaration.text("$")?;
        let mut diagnostics = Vec::new();
        for finding in self.findings {
            let mut record = scalar_record(finding, None)?;
            record.provenance(self.root, self.root, declaration)?;
            diagnostics.push(record.materialize());
        }
        Ok(diagnostics)
    }
}
/// Consume source descriptors once. Semantic and opaque capture aggregation is
/// unchanged: this report includes scalar failures only, never replacing other
/// original failed captures or deciding their later boundary projection. The
/// project file declares every failed file.
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
            scalar_record(finding, Some(source.capture.origin))?.provenance(
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
                let mut record = scalar_record(finding, Some(source.capture.origin))?;
                record.provenance(source.capture.identity, self.root, declaration)?;
                diagnostics.push(record.materialize());
            }
        }
        Ok(diagnostics)
    }
}
