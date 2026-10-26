//! The roots of a project's own files: the project file, a function file and
//! a codelist file (#2844, design section 2).
//!
//! Each root owns the path arena of its file. A project's `functions` lists
//! function files, each naming its own function with a callable for each
//! host language, and `environments.<host>.lock` holds each host's lock
//! (D8). An inline codelist source is a root of its own, with its own arena,
//! inside its declaration.
//!
//! Values are held as the engine reads them: a front end converts each
//! written scalar of a definition or a codelist and reports one it cannot,
//! as these files always have.

use super::{Header, Meta, Named, Paths, Text};
use crate::cdisc_vocabulary::{DefineContext, GuideStatus};
use crate::project_environment::Submission;
use crate::project_function::Language;
use crate::value::{ColumnType, Value, ValueType};

/// A project file (PROJECT-1).
#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub header: Header,
    /// The function files of the project functions specifications may call
    /// (FUNC-1, FUNC-4). `None` when the project does not write the field.
    pub functions: Option<Vec<FunctionDeclaration>>,
    /// The environment of each host language the project declares, by
    /// language key (PROJECT-2).
    pub environments: Vec<HostEnvironment>,
    /// Codelist sources supplying study terminology, in written order (CT-1).
    pub codelists: Vec<CodelistDeclaration>,
    /// The study and the submission sections, or why they cannot be read.
    /// A run reads a section's specifications; only Define-XML composition
    /// requires the rest.
    pub submission: Result<SubmissionDeclarations, SubmissionRefused>,
    pub paths: Paths,
    pub meta: Meta,
}

impl Project {
    /// The environment the project declares for `language`.
    pub fn environment(&self, language: Language) -> Option<&HostEnvironment> {
        self.environments
            .iter()
            .find(|environment| environment.language == language)
    }
}

/// The study and the sections of a project whose declarations are complete.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubmissionDeclarations {
    pub study: Option<Study>,
    /// In written order (PROJECT-3).
    pub sections: Vec<Section>,
}

/// The study and section declarations do not have the closed shape the
/// language requires. The schema admits only complete declarations, so
/// only a document admitted without it can be refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubmissionRefused;

/// One host language's environment (PROJECT-2).
#[derive(Clone, Debug, PartialEq)]
pub struct HostEnvironment {
    pub language: Language,
    /// The packaging lock the installed called packages are verified
    /// against (FUNC-3): `environments.<host>.lock`, relative to the project
    /// file.
    pub lock: Option<Text>,
    pub meta: Meta,
}

/// One function file a project lists in `functions`, relative to the
/// project file (D8). The file names its own function.
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionDeclaration {
    pub file: Text,
    pub meta: Meta,
}

/// One declared codelist source.
#[derive(Clone, Debug, PartialEq)]
pub struct CodelistDeclaration {
    pub source: CodelistSource,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CodelistSource {
    /// A source written in the project file.
    Inline(Box<Codelists>),
    /// A codelist file, relative to the project file.
    File(Text),
}

/// One project function (FUNC-4): a function file.
#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    /// The function file's own header.
    pub header: Header,
    pub name: Named,
    pub description: Text,
    /// Closed and ordered (FUNC-5).
    pub params: Vec<Parameter>,
    pub returns: ColumnType,
    pub may_return_missing: bool,
    /// Decimal places at which numeric test results are compared (FUNC-13).
    pub comparison_decimals: i64,
    /// The callable each host language binds (FUNC-8): a function file
    /// writes one for `python`, `r` or both.
    pub implementations: Vec<Implementation>,
    /// Activation runs these before any study data is read (FUNC-11).
    pub tests: Vec<FunctionTest>,
    pub paths: Paths,
    pub meta: Meta,
}

impl Function {
    /// The callable `language` binds, if the definition writes one.
    pub fn implementation(&self, language: Language) -> Option<&Implementation> {
        self.implementations
            .iter()
            .find(|implementation| implementation.language == language)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub name: Named,
    pub kind: ValueType,
    pub required: bool,
    /// The value an omitted optional argument takes (FUNC-7).
    pub default: Option<Value>,
    pub accepts_missing: bool,
    pub meta: Meta,
}

/// A statically written fully qualified callable (FUNC-8).
#[derive(Clone, Debug, PartialEq)]
pub struct Implementation {
    pub language: Language,
    pub callable: Text,
}

/// One inline test case (FUNC-11).
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionTest {
    pub id: Text,
    /// The obligations the case demonstrates, as written.
    pub covers: Vec<Text>,
    /// The logical arguments by parameter name, in written order.
    pub args: Vec<(Named, Value)>,
    pub result: Value,
    pub meta: Meta,
}

/// One codelist source (CT-1): a codelist file, or a source written in the
/// project file.
#[derive(Clone, Debug, PartialEq)]
pub struct Codelists {
    /// A codelist file's own header; a source written in a project file has
    /// none.
    pub header: Option<Header>,
    /// The published terminology the codelists are drawn from; a source
    /// without one is sponsor-defined (CT-2).
    pub standard: Option<TerminologyStandard>,
    pub codelists: Vec<Codelist>,
    pub paths: Paths,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerminologyStandard {
    pub name: Text,
    pub publishing_set: Text,
    pub version: Text,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Codelist {
    /// The name column bindings refer to the codelist by.
    pub id: Named,
    pub name: Text,
    pub data_type: CodelistType,
    /// Whether the published list admits sponsor items marked extended (CT-6).
    pub extensible: bool,
    pub alias: Option<Text>,
    /// A transport format name (CT-12).
    pub format_name: Option<Text>,
    /// The whole value set of a bound column, in written order (CT-7).
    pub items: Option<Vec<CodelistItem>>,
    pub external: Option<ExternalDictionary>,
    pub meta: Meta,
}

/// The type of a codelist's coded values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodelistType {
    Text,
    Integer,
    Float,
}

impl CodelistType {
    /// The column type a bound column must have.
    pub fn column_type(self) -> ColumnType {
        match self {
            Self::Text => ColumnType::Str,
            Self::Integer => ColumnType::Int,
            Self::Float => ColumnType::Float,
        }
    }
    /// The type as `data_type` writes it, and as Define-XML writes a code
    /// list's `DataType`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Integer => "integer",
            Self::Float => "float",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodelistItem {
    pub value: Value,
    pub decode: Option<Text>,
    pub rank: Option<i64>,
    pub alias: Option<Text>,
    /// Whether the sponsor added the value to an extensible published list.
    pub extended: bool,
    pub meta: Meta,
}

/// The dictionary a codelist defers to instead of listing values.
#[derive(Clone, Debug, PartialEq)]
pub struct ExternalDictionary {
    pub dictionary: Text,
    pub version: Text,
    pub href: Option<Text>,
    pub meta: Meta,
}

/// The study the specifications belong to (DEFINE-1).
#[derive(Clone, Debug, PartialEq)]
pub struct Study {
    pub id: Named,
    pub name: Text,
    pub description: Text,
    pub protocol_name: Text,
    pub meta: Meta,
}

/// The selected Define-XML release; omission retains the 2.1 maintenance release.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DefineVersion {
    V2_0,
    #[default]
    V2_1,
}
impl DefineVersion {
    pub const VALUES: &'static [(&'static str, Self)] =
        &[("2.0.0", Self::V2_0), ("2.1.11", Self::V2_1)];
    pub fn name(self) -> &'static str {
        match self {
            Self::V2_0 => "2.0.0",
            Self::V2_1 => "2.1.11",
        }
    }
    pub fn namespace(self) -> &'static str {
        match self {
            Self::V2_0 => "http://www.cdisc.org/ns/def/v2.0",
            Self::V2_1 => "http://www.cdisc.org/ns/def/v2.1",
        }
    }
    pub fn family(self) -> &'static str {
        match self {
            Self::V2_0 => "2.0",
            Self::V2_1 => "2.1",
        }
    }
}

/// One selected submission and its publication declarations.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub family: Submission,
    /// The ordered specifications; the only field a run needs.
    pub specs: Vec<Text>,
    /// The implementation guides binding the datasets (DEFINE-6), the first
    /// the default (D16).
    pub standards: Vec<ImplementationGuide>,
    /// The Define-XML target, relative to the project file (DEFINE-2).
    pub define: Option<Text>,
    pub define_version: DefineVersion,
    /// Supporting documents (DEFINE-13).
    pub documents: Vec<SupportingDocument>,
    /// The transport file written for each composed dataset (JSON-1,
    /// XPT-1).
    pub transport: Option<Transport>,
    /// `def:Context` of the generated document (DEFINE-4).
    pub context: DefineContext,
    /// The language of generated translated text; `en` when omitted.
    pub xml_lang: Text,
    pub file_oid: Option<Text>,
    pub metadata_version: Option<MetadataVersion>,
    pub originator: Option<Text>,
    pub stylesheet: Option<Text>,
    /// An authored source-system version; omission selects the installed
    /// package version (DEFINE-12).
    pub source_system_version: Option<Text>,
    pub meta: Meta,
}

/// An implementation guide. Its name is a closed CDISC vocabulary, which
/// stays text (P-VOC-1).
#[derive(Clone, Debug, PartialEq)]
pub struct ImplementationGuide {
    pub name: Text,
    pub version: Text,
    pub status: GuideStatus,
    pub meta: Meta,
}

/// The one transport file a section writes for each composed dataset: a SAS
/// Transport file, or a Dataset-JSON 1.1 file in the representation the
/// section selects (JSON-4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    Xpt,
    /// Dataset-JSON in its single-object form.
    DatasetJson,
    /// Dataset-JSON in its newline-delimited form: the metadata object on
    /// the first line, then one row per line.
    DatasetNdjson,
}

impl Transport {
    /// The transport file's extension, after the lower-case dataset name.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Xpt => "xpt",
            Self::DatasetJson => "json",
            Self::DatasetNdjson => "ndjson",
        }
    }
}

/// A document a section's datasets may reference (DEFINE-13).
#[derive(Clone, Debug, PartialEq)]
pub struct SupportingDocument {
    pub id: Named,
    pub kind: DocumentKind,
    pub link: Text,
    pub title: Text,
    pub meta: Meta,
}

/// Where a supporting document is listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    AnnotatedCrf,
    Supplemental,
    /// Listed only where it is referenced.
    Other,
}

/// Overrides of the generated metadata version (DEFINE-3).
#[derive(Clone, Debug, PartialEq)]
pub struct MetadataVersion {
    pub id: Named,
    pub name: Text,
    pub description: Option<Text>,
    pub meta: Meta,
}
