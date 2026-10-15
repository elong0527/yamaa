//! Borrowed environment reports admitted before the first owned diagnostic fact.
//!
//! The caller supplies the exact retained failed draft, normalized root and
//! captures from one attempt. Matching identity strings or equal arena contents
//! cannot establish that ownership. Origin checks validate the supplied retained
//! relationships; they do not manufacture provenance or authorize source reads.
use super::{finding_diagnostic, scalar_diagnostic, Error as CoreError, Project};
use crate::{
    diagnostic::{ConditionCode, ContextValue, Diagnostic},
    project_environment::{Draft, Finding},
    project_function::{Finding as FunctionFinding, Language},
    project_function_document::Finding as ScalarFinding,
    schema::{Document, DocumentNode as N},
    value::Value,
};
use alloc::{string::String, vec, vec::Vec};
use core::fmt::Write;

// This preserves the pre-existing adapter projection ceiling, independently of
// project_source's configurable capture admission. Inline functions and findings
// are not capture descriptors; their report capacity is not reduced to 128.
const MAX_CAPTURES: usize = 128;
const MAX_FINDINGS: usize = 65_536;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// The historical raw semantic text policy, independently of issue encoding.
    pub semantic_text_bytes: usize,
    /// Conservative admission for five-field issue rows with JSON-text context.
    pub issue_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            semantic_text_bytes: 16_777_216,
            issue_bytes: 16_777_216,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Core(CoreError),
    InvalidOrigin,
    IssueLimit,
}
impl From<CoreError> for Error {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}
#[derive(Clone, Copy)]
pub struct Root<'a> {
    pub document: &'a Document,
    pub identity: &'a str,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Origin<'a> {
    Function(&'a str),
    Codelist(usize),
}
#[derive(Clone, Copy)]
pub struct Capture<'a> {
    pub origin: Origin<'a>,
    pub document: &'a Document,
    pub identity: &'a str,
}
/// Origin slices and captures are borrowed from the same retained attempt as
/// `draft` and `root`. Admission retains at most 128 borrowed descriptors in a
/// fixed array, so an arbitrary input iterator is never replayed.
pub struct Admission<'a, I> {
    pub root: Root<'a>,
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
    root: Root<'a>,
    draft: &'a Draft,
    function_origins: &'a [String],
    codelist_origins: &'a [usize],
    captures: [Option<Capture<'a>>; MAX_CAPTURES],
    findings: &'a [Finding],
}
pub struct RootScalarReport<'a> {
    root: Root<'a>,
    findings: &'a [ScalarFinding],
    header: [Option<Finding>; 4],
}
pub struct ScalarSourcesReport<'a> {
    root: Root<'a>,
    sources: [Option<ScalarSource<'a>>; MAX_CAPTURES],
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
    fn bytes(&self) -> Result<usize, Error> {
        self.pieces[..self.len]
            .iter()
            .flatten()
            .try_fold(0usize, |total, piece| {
                total
                    .checked_add(match piece {
                        Piece::Text(text) => text.len(),
                        Piece::Index(n) => digits(*n) + 2,
                    })
                    .ok_or(Error::IssueLimit)
            })
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
fn digits(mut n: usize) -> usize {
    let mut count = 1;
    while n >= 10 {
        n /= 10;
        count += 1;
    }
    count
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
                    FieldValue::Text(text) => ContextValue::Scalar(Value::Str(text.into())),
                    FieldValue::Scalar(value) => ContextValue::Scalar(value.clone()),
                    FieldValue::Integer(text) => ContextValue::Integer(text.into()),
                    FieldValue::Path(path) => ContextValue::Scalar(Value::Str(path.render())),
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
struct SemanticBudget(usize);
impl SemanticBudget {
    fn charge(&mut self, bytes: usize) -> Result<(), Error> {
        self.0 = self
            .0
            .checked_sub(bytes)
            .ok_or(Error::Core(CoreError::Limit))?;
        Ok(())
    }
    fn record(&mut self, record: &Record<'_>) -> Result<(), Error> {
        self.charge(128)?;
        self.charge(
            record
                .path
                .bytes()
                .map_err(|_| Error::Core(CoreError::Limit))?,
        )?;
        for field in record.fields[..record.len].iter().flatten() {
            self.charge(field.name.len())?;
            self.charge(match field.value {
                FieldValue::Text(text) | FieldValue::Integer(text) => text.len(),
                FieldValue::Scalar(Value::Str(text)) => text.len(),
                FieldValue::Scalar(_) => 32,
                FieldValue::Path(_) => return Err(Error::Core(CoreError::UnavailableShape)),
            })?;
        }
        Ok(())
    }
}
struct IssueBudget(usize);
impl IssueBudget {
    fn charge(&mut self, bytes: usize) -> Result<(), Error> {
        self.0 = self.0.checked_sub(bytes).ok_or(Error::IssueLimit)?;
        Ok(())
    }
    fn bytes(&mut self, bytes: usize, expansion: usize) -> Result<(), Error> {
        self.charge(bytes.checked_mul(expansion).ok_or(Error::IssueLimit)?)
    }
    fn text(&mut self, text: &str, expansion: usize) -> Result<(), Error> {
        self.bytes(text.len(), expansion)
    }
    fn path(&mut self, path: &Path<'_>, expansion: usize) -> Result<(), Error> {
        self.bytes(path.bytes()?, expansion)
    }
    fn record(&mut self, record: &Record<'_>) -> Result<(), Error> {
        self.charge(512)?;
        self.path(&record.path, 6)?;
        for field in record.fields[..record.len].iter().flatten() {
            self.text(field.name, 12)?;
            match field.value {
                FieldValue::Text(text) => self.text(text, 12)?,
                FieldValue::Scalar(Value::Str(text)) => self.text(text, 12)?,
                FieldValue::Integer(text) => self.text(text, 2)?,
                FieldValue::Scalar(_) => self.charge(64)?,
                FieldValue::Path(_) => return Err(Error::Core(CoreError::UnavailableShape)),
            }
        }
        Ok(())
    }
    fn provenance(
        &mut self,
        source: &str,
        entry: &str,
        declaration: &Path<'_>,
    ) -> Result<(), Error> {
        self.text(source, 12)?;
        self.text(entry, 12)?;
        self.path(declaration, 12)
    }
}
fn root_mapping(root: Root<'_>) -> Result<(), Error> {
    if !matches!(
        root.document.nodes().get(root.document.root()),
        Some(N::Mapping(_))
    ) {
        return Err(Error::Core(CoreError::UnavailableShape));
    }
    Ok(())
}
fn function_node<'a>(root: Root<'a>, name: &str) -> Result<usize, Error> {
    let node = root
        .document
        .field(root.document.root(), "functions")
        .ok_or(Error::InvalidOrigin)?;
    let Some(N::Mapping(entries)) = root.document.nodes().get(node) else {
        return Err(Error::InvalidOrigin);
    };
    entries
        .iter()
        .find_map(|&(key, value)| match &root.document.nodes()[key] {
            N::Text(text) if text == name => Some(value),
            _ => None,
        })
        .ok_or(Error::InvalidOrigin)
}
fn codelist_node(root: Root<'_>, index: usize) -> Result<usize, Error> {
    let node = root
        .document
        .field(root.document.root(), "codelists")
        .ok_or(Error::InvalidOrigin)?;
    let Some(N::Sequence(entries)) = root.document.nodes().get(node) else {
        return Err(Error::InvalidOrigin);
    };
    entries.get(index).copied().ok_or(Error::InvalidOrigin)
}
fn authored_node(root: Root<'_>, origin: Origin<'_>) -> Result<usize, Error> {
    match origin {
        Origin::Function(name) => function_node(root, name),
        Origin::Codelist(index) => codelist_node(root, index),
    }
}
fn declaration<'a>(origin: Origin<'a>) -> Result<Path<'a>, CoreError> {
    let mut path = Path::new();
    match origin {
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
    root_mapping(input.root)?;
    let functions = input.draft.functions.as_deref().unwrap_or(&[]);
    if input.function_origins.len() != functions.len()
        || input.codelist_origins.len() != input.draft.codelists.len()
        || input
            .function_origins
            .iter()
            .zip(functions)
            .any(|(name, function)| name != &function.name)
    {
        return Err(Error::InvalidOrigin);
    }
    if input.captures.clone().take(MAX_CAPTURES + 1).count() > MAX_CAPTURES {
        return Err(Error::IssueLimit);
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
        if !matches!(
            input
                .root
                .document
                .nodes()
                .get(authored_node(input.root, capture.origin)?),
            Some(N::Text(_))
        ) || !matches!(
            capture.document.nodes().get(capture.document.root()),
            Some(N::Mapping(_))
        ) {
            return Err(Error::InvalidOrigin);
        }
        let included = match capture.origin {
            Origin::Function(name) => input.function_origins.iter().any(|origin| origin == name),
            Origin::Codelist(index) => input.codelist_origins.contains(&index),
        };
        if !included {
            return Err(Error::InvalidOrigin);
        }
    }
    let entries = input
        .root
        .document
        .field(input.root.document.root(), "functions")
        .and_then(|node| input.root.document.nodes().get(node));
    let mut authored = match entries {
        Some(N::Mapping(entries)) => entries.iter(),
        None if input.function_origins.is_empty() => [].iter(),
        _ => return Err(Error::InvalidOrigin),
    };
    for name in input.function_origins {
        // Successful typed declarations are an ordered subsequence of authored
        // declarations. Failed captures leave holes, never reordered identities.
        let (_, node) = authored.by_ref().find(|&&(key, _)| matches!(&input.root.document.nodes()[key], N::Text(text) if text == name)).ok_or(Error::InvalidOrigin)?;
        let external = capture_for(&input.captures, Origin::Function(name)).is_some();
        if !matches!(
            (input.root.document.nodes().get(*node), external),
            (Some(N::Text(_)), true) | (Some(N::Mapping(_)), false)
        ) {
            return Err(Error::InvalidOrigin);
        }
    }
    for (typed, &index) in input.codelist_origins.iter().enumerate() {
        if typed > 0 && input.codelist_origins[typed - 1] >= index {
            return Err(Error::InvalidOrigin);
        }
        let external = capture_for(&input.captures, Origin::Codelist(index)).is_some();
        if !matches!(
            (
                input
                    .root
                    .document
                    .nodes()
                    .get(codelist_node(input.root, index)?),
                external
            ),
            (Some(N::Text(_)), true) | (Some(N::Mapping(_)), false)
        ) {
            return Err(Error::InvalidOrigin);
        }
    }
    Ok(())
}
fn admission_record<'a, I: Iterator<Item = Capture<'a>> + Clone>(
    input: &Admission<'a, I>,
    finding: &'a Finding,
    issue: Option<&mut IssueBudget>,
    semantic: Option<&mut SemanticBudget>,
) -> Result<Record<'a>, Error> {
    let mut record = finding_diagnostic(input.draft, finding, &mut Borrowed)?;
    if let Some(semantic) = semantic {
        semantic.record(&record)?;
    }
    let raw_path = record.path;
    let (source, declaration, extra) = match finding {
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
                external.map_or(input.root.identity, |capture| capture.identity),
                declaration(origin)?,
                Some(name.as_str()),
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
                external.map_or(input.root.identity, |capture| capture.identity),
                declaration(origin)?,
                None,
            )
        }
        _ => (input.root.identity, raw_path, None),
    };
    if let Some(issue) = issue {
        // Preserve the historical distinct issue policy's raw and rebased paths.
        let selected_path = record.path;
        record.path = raw_path;
        issue.record(&record)?;
        record.path = selected_path;
        match finding {
            Finding::Function { .. } => {
                if let Some(name) = extra {
                    issue.text(name, 18)?;
                }
                issue.charge(128)?;
                issue.path(&record.path, 6)?;
            }
            Finding::Codelist(_) => {
                issue.charge(128)?;
                let mut suffix = raw_path.tail(2);
                let Some(Piece::Text(first)) = suffix.pieces[0] else {
                    return Err(Error::Core(CoreError::UnavailableShape));
                };
                suffix.pieces[0] = Some(Piece::Text(
                    first
                        .strip_prefix('.')
                        .ok_or(Error::Core(CoreError::UnavailableShape))?,
                ));
                issue.path(&suffix, 6)?;
            }
            _ => issue.path(&raw_path, 18)?,
        }
        issue.provenance(source, input.root.identity, &declaration)?;
    }
    record.provenance(source, input.root.identity, declaration)?;
    Ok(record)
}
fn retained<T: Copy>(input: impl Iterator<Item = T>) -> Result<[Option<T>; MAX_CAPTURES], Error> {
    let mut items = [None; MAX_CAPTURES];
    for (index, item) in input.enumerate() {
        *items.get_mut(index).ok_or(Error::IssueLimit)? = Some(item);
    }
    Ok(items)
}
/// Consume the supplied capture iterator once, then admit the complete ordered
/// static report using immutable, bounded borrowed descriptors.
pub fn admit_admission<'a, I: Iterator<Item = Capture<'a>>>(
    input: Admission<'a, I>,
    findings: &'a [Finding],
    limits: Limits,
) -> Result<AdmissionReport<'a>, Error> {
    let captures = retained(input.captures)?;
    let input = Admission {
        root: input.root,
        draft: input.draft,
        function_origins: input.function_origins,
        codelist_origins: input.codelist_origins,
        captures: captures.iter().flatten().copied(),
    };
    validate_admission(&input)?;
    if findings.len() > MAX_FINDINGS {
        return Err(Error::Core(CoreError::Limit));
    }
    let mut semantic = SemanticBudget(limits.semantic_text_bytes);
    let mut issue = IssueBudget(limits.issue_bytes);
    for finding in findings {
        admission_record(&input, finding, Some(&mut issue), Some(&mut semantic))?;
    }
    Ok(AdmissionReport {
        root: input.root,
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
            root: self.root,
            draft: self.draft,
            function_origins: self.function_origins,
            codelist_origins: self.codelist_origins,
            captures: self.captures.iter().flatten().copied(),
        };
        self.findings
            .iter()
            .map(|finding| admission_record(&input, finding, None, None).map(Record::materialize))
            .collect()
    }
}

fn headers(document: &Document, host: Option<Language>) -> Result<[Option<Finding>; 4], Error> {
    let Some(host) = host else {
        return Ok([None, None, None, None]);
    };
    super::root_findings(document, host).map_err(Error::Core)
}
fn root_record<'a>(root: Root<'a>, finding: &'a ScalarFinding) -> Result<Record<'a>, Error> {
    scalar_diagnostic(root.document, finding, &mut Borrowed).map_err(Error::Core)
}
fn scalar_admit<'a>(
    mut record: Record<'a>,
    source: &'a str,
    root: Root<'a>,
    declaration: Path<'a>,
    semantic: &mut SemanticBudget,
    issue: &mut IssueBudget,
) -> Result<(), Error> {
    semantic.record(&record)?;
    issue.record(&record)?;
    issue.provenance(source, root.identity, &declaration)?;
    record.provenance(source, root.identity, declaration)?;
    Ok(())
}
pub fn admit_root_scalars<'a>(
    root: Root<'a>,
    findings: &'a [ScalarFinding],
    host: Option<Language>,
    limits: Limits,
) -> Result<RootScalarReport<'a>, Error> {
    if findings.len() > MAX_FINDINGS {
        return Err(Error::Core(CoreError::Limit));
    }
    let header = headers(root.document, host)?;
    let draft = Draft {
        language: None,
        lock: None,
        functions: None,
        codelists: Vec::new(),
        has_study: false,
        submissions: Vec::new(),
    };
    let mut semantic = SemanticBudget(limits.semantic_text_bytes);
    let mut issue = IssueBudget(limits.issue_bytes);
    let mut declaration = Path::new();
    declaration.text("$")?;
    for finding in header.iter().flatten() {
        scalar_admit(
            finding_diagnostic(&draft, finding, &mut Borrowed)?,
            root.identity,
            root,
            declaration,
            &mut semantic,
            &mut issue,
        )?;
    }
    // Root header and scalar decoding retain their separate semantic text limits.
    let mut semantic = SemanticBudget(limits.semantic_text_bytes);
    for finding in findings {
        scalar_admit(
            root_record(root, finding)?,
            root.identity,
            root,
            declaration,
            &mut semantic,
            &mut issue,
        )?;
    }
    Ok(RootScalarReport {
        root,
        findings,
        header,
    })
}
impl RootScalarReport<'_> {
    pub fn diagnostics(&self) -> Result<Vec<Diagnostic>, Error> {
        let draft = Draft {
            language: None,
            lock: None,
            functions: None,
            codelists: Vec::new(),
            has_study: false,
            submissions: Vec::new(),
        };
        let mut declaration = Path::new();
        declaration.text("$")?;
        let mut diagnostics = Vec::new();
        for finding in self.header.iter().flatten() {
            let mut record = finding_diagnostic(&draft, finding, &mut Borrowed)?;
            record.provenance(self.root.identity, self.root.identity, declaration)?;
            diagnostics.push(record.materialize());
        }
        for finding in self.findings {
            let mut record = root_record(self.root, finding)?;
            record.provenance(self.root.identity, self.root.identity, declaration)?;
            diagnostics.push(record.materialize());
        }
        Ok(diagnostics)
    }
}
/// Consume source descriptors once. Semantic and opaque capture aggregation is
/// unchanged: this report includes scalar failures only, never replacing other
/// original failed captures or deciding their later boundary projection.
pub fn admit_scalar_sources<'a, I: Iterator<Item = ScalarSource<'a>>>(
    root: Root<'a>,
    sources: I,
    limits: Limits,
) -> Result<ScalarSourcesReport<'a>, Error> {
    root_mapping(root)?;
    let sources = retained(sources)?;
    let mut issue = IssueBudget(limits.issue_bytes);
    for (index, source) in sources.iter().flatten().enumerate() {
        if sources[..index]
            .iter()
            .flatten()
            .any(|earlier| earlier.capture.origin == source.capture.origin)
            || !matches!(
                root.document
                    .nodes()
                    .get(authored_node(root, source.capture.origin)?),
                Some(N::Text(_))
            )
        {
            return Err(Error::InvalidOrigin);
        }
        if source.findings.len() > MAX_FINDINGS {
            return Err(Error::Core(CoreError::Limit));
        }
        issue.charge(128)?;
        if let Origin::Function(name) = source.capture.origin {
            issue.text(name, 12)?;
        }
        let declaration = declaration(source.capture.origin)?;
        let mut semantic = SemanticBudget(limits.semantic_text_bytes);
        for finding in source.findings {
            scalar_admit(
                scalar_diagnostic(source.capture.document, finding, &mut Borrowed)?,
                source.capture.identity,
                root,
                declaration,
                &mut semantic,
                &mut issue,
            )?;
        }
    }
    Ok(ScalarSourcesReport { root, sources })
}
impl ScalarSourcesReport<'_> {
    pub fn diagnostics(&self) -> Result<Vec<Diagnostic>, Error> {
        let mut diagnostics = Vec::new();
        for source in self.sources.iter().flatten() {
            let declaration = declaration(source.capture.origin)?;
            for finding in source.findings {
                let mut record =
                    scalar_diagnostic(source.capture.document, finding, &mut Borrowed)?;
                record.provenance(source.capture.identity, self.root.identity, declaration)?;
                diagnostics.push(record.materialize());
            }
        }
        Ok(diagnostics)
    }
}
