//! The roots of a project's own files: the project file, a function file and
//! a codelist file (#2844, design section 2).
//!
//! Each root owns the path arena of its file. A 0.5 project links one
//! environment file per host language; the one its host reads is lowered
//! into the host's project root: its lock into that language's entry of
//! [`Project::environments`] and its definitions into
//! [`Project::functions`]. A 1.0 project writes both itself: `functions`
//! lists function files, each naming its own function with a callable for
//! each host language, and `environments.<host>.lock` holds each host's lock
//! (D8). An inline definition or codelist source is a root of its own, with
//! its own arena, inside its declaration.
//!
//! Values are held as the engine reads them: a front end converts each
//! written scalar of a definition or a codelist and reports one it cannot,
//! as these files always have.

use super::{Header, Meta, Named, Paths, Text};
use crate::project_environment::Submission;
use crate::project_function::Language;
use crate::value::{ColumnType, Value, ValueType};

/// A project file (PROJECT-1).
#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub header: Header,
    /// The project functions specifications may call (FUNC-1, FUNC-4); in
    /// 0.5 those of the host's environment file. `None` when nothing
    /// declares the field.
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
    /// 0.5: the environment file the language key names, relative to the
    /// project file. Its lock and function paths resolve from that file. A
    /// 1.0 project has no environment file.
    pub file: Option<Text>,
    /// The packaging lock the installed called packages are verified
    /// against (FUNC-3): in 1.0 `environments.<host>.lock`, relative to the
    /// project file; in 0.5 the linked environment file's `lock`.
    pub lock: Option<Text>,
    pub meta: Meta,
}

/// One declared project function.
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionDeclaration {
    /// The logical name a 0.5 environment file keys the definition by; a
    /// function file, which a 1.0 project lists, names its own function.
    pub key: Option<Named>,
    pub source: FunctionSource,
    pub meta: Meta,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FunctionSource {
    /// A definition written in place, as a 0.5 environment file may.
    Inline(Box<Function>),
    /// A function file, relative to the file that declares it: the 0.5
    /// environment file, or the 1.0 project file.
    File(Text),
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

/// One project function (FUNC-4): a function file, or a definition written
/// in a 0.5 environment file.
#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    /// A 1.0 function file's own header; a 0.5 definition has none.
    pub header: Option<Header>,
    pub name: Named,
    pub description: Text,
    /// Closed and ordered (FUNC-5).
    pub params: Vec<Parameter>,
    pub returns: ColumnType,
    pub may_return_missing: bool,
    /// Decimal places at which numeric test results are compared (FUNC-13).
    pub comparison_decimals: i64,
    /// The callable each host language binds (FUNC-8): a 1.0 function file
    /// writes one for `python`, `r` or both, and a 0.5 definition one, for
    /// the language of the environment file declaring it.
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
    /// A 1.0 codelist file's own header; a source written in a project
    /// file, and every 0.5 source, has none.
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

/// One submission section (PROJECT-3): its specifications and the metadata
/// composing them into one Define-XML document.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub family: Submission,
    /// The ordered specifications; the only field a run needs.
    pub specs: Vec<Text>,
    /// The implementation guides binding the datasets (DEFINE-6). 0.5
    /// writes at most one, as `standard`.
    pub standards: Vec<ImplementationGuide>,
    /// The Define-XML target, relative to the project file (DEFINE-2).
    pub define: Option<Text>,
    /// Supporting documents (DEFINE-13).
    pub documents: Vec<SupportingDocument>,
    /// The transport file written for each composed dataset (JSON-1,
    /// XPT-1); 0.5 writes it as `datasets`.
    pub transport: Option<Transport>,
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

/// Every standard name Define-XML 2.1 publishes, in its order: the closed
/// vocabulary of an implementation guide's and a terminology source's name,
/// which `yaml/vocabulary/cdisc.yaml` holds (DEFINE-6, CT-2).
pub const STANDARD_NAMES: &[&str] = &[
    "ADaMIG",
    "ADaMIG-MD",
    "BIMO",
    "CDISC/NCI",
    "SDTMIG",
    "SDTMIG-AP",
    "SDTMIG-MD",
    "SENDIG",
    "SENDIG-AR",
    "SENDIG-DART",
    "SENDIG-GENETOX",
];

/// An implementation guide. Its name is a closed CDISC vocabulary, which
/// stays text (P-VOC-1).
#[derive(Clone, Debug, PartialEq)]
pub struct ImplementationGuide {
    pub name: Text,
    pub version: Text,
    pub status: GuideStatus,
    pub meta: Meta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideStatus {
    Draft,
    Final,
    /// Admitted by a 1.0 project file only (P-STD-1); no 0.5 form.
    Provisional,
}

impl GuideStatus {
    /// The status as Define-XML writes it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Final => "Final",
            Self::Provisional => "Provisional",
        }
    }
}

/// `def:Context` of the generated document (DEFINE-4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefineContext {
    Submission,
    Other,
}

/// The one transport file a section writes for each composed dataset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    Xpt,
    DatasetJson,
}

impl Transport {
    /// The transport file's extension, after the lower-case dataset name.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Xpt => "xpt",
            Self::DatasetJson => "json",
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
