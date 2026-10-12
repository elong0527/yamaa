//! Closed declaration values and normalized occurrence facts. None grants IO.
use crate::{diagnostic::ScalarSubmissionRule, project_environment::Submission};
use alloc::{string::String, vec::Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldName {
    Study,
    Sdtm,
    Adam,
    Send,
    Id,
    Name,
    Description,
    ProtocolName,
    Standard,
    Version,
    Status,
    Specs,
    Define,
    Documents,
    DatasetJson,
    Context,
    XmlLang,
    FileOid,
    MetadataVersion,
    Originator,
    Stylesheet,
    SourceSystemVersion,
    Kind,
    Link,
    Title,
}
impl FieldName {
    pub fn name(self) -> &'static str {
        match self {
            Self::Study => "study",
            Self::Sdtm => "sdtm",
            Self::Adam => "adam",
            Self::Send => "send",
            Self::Id => "id",
            Self::Name => "name",
            Self::Description => "description",
            Self::ProtocolName => "protocol_name",
            Self::Standard => "standard",
            Self::Version => "version",
            Self::Status => "status",
            Self::Specs => "specs",
            Self::Define => "define",
            Self::Documents => "documents",
            Self::DatasetJson => "dataset_json",
            Self::Context => "context",
            Self::XmlLang => "xml_lang",
            Self::FileOid => "file_oid",
            Self::MetadataVersion => "metadata_version",
            Self::Originator => "originator",
            Self::Stylesheet => "stylesheet",
            Self::SourceSystemVersion => "source_system_version",
            Self::Kind => "kind",
            Self::Link => "link",
            Self::Title => "title",
        }
    }
}
/// The container/key/value occurrences of one normalized field. A missing key
/// and value identify an existing schema default, not an authored occurrence.
/// These are document facts, not filesystem declaring-origin claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Origin {
    pub container: usize,
    pub key: Option<usize>,
    pub node: Option<usize>,
    pub field: FieldName,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field<T> {
    pub value: T,
    pub origin: Origin,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Field(FieldName),
    Index(usize),
}
/// A closed, fixed-capacity field path needs no unbounded diagnostic strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Path {
    pub(super) steps: [Option<Step>; 6],
    pub(super) len: usize,
}
impl Path {
    pub fn steps(&self) -> impl Iterator<Item = Step> + '_ {
        self.steps[..self.len].iter().flatten().copied()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Missing,
    ExpectedMapping,
    ExpectedSequence,
    ExpectedText,
    ExpectedBoolean,
    NonTextKey,
    UnknownField,
    EmptyText,
    InvalidIdentifier,
    InvalidDefineId,
    InvalidHref,
    InvalidLanguage,
    InvalidChoice,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub node: usize,
    /// Even unknown or non-text keys retain their exact field occurrences.
    pub container: Option<usize>,
    pub key: Option<usize>,
    pub origin: Option<Origin>,
    pub path: Path,
    pub kind: Kind,
}
impl Finding {
    /// Only scalar rules whose current normative causes are known are portable
    /// here. Shape/transport and unsupported choices retain their original facts.
    pub fn scalar_rule(&self) -> Option<ScalarSubmissionRule> {
        match self.kind {
            Kind::InvalidIdentifier => Some(ScalarSubmissionRule::Identifier),
            Kind::InvalidDefineId => Some(ScalarSubmissionRule::DefineId),
            Kind::InvalidHref => Some(ScalarSubmissionRule::Href),
            Kind::InvalidLanguage => Some(ScalarSubmissionRule::Language),
            Kind::InvalidChoice
                if self.origin.map(|v| v.field) == Some(FieldName::Name)
                    && self
                        .path
                        .steps()
                        .any(|v| v == Step::Field(FieldName::Standard)) =>
            {
                Some(ScalarSubmissionRule::StandardName)
            }
            _ => None,
        }
    }
    /// Compatibility view of the original optional scalar citation.
    pub fn requirement(&self) -> Option<&'static str> {
        self.scalar_rule().map(ScalarSubmissionRule::requirement)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideName {
    AdamIg,
    AdamIgMd,
    Bimo,
    CdiscNci,
    SdtmIg,
    SdtmIgAp,
    SdtmIgMd,
    SendIg,
    SendIgAr,
    SendIgDart,
    SendIgGenetox,
}
impl GuideName {
    pub fn name(self) -> &'static str {
        match self {
            Self::AdamIg => "ADaMIG",
            Self::AdamIgMd => "ADaMIG-MD",
            Self::Bimo => "BIMO",
            Self::CdiscNci => "CDISC/NCI",
            Self::SdtmIg => "SDTMIG",
            Self::SdtmIgAp => "SDTMIG-AP",
            Self::SdtmIgMd => "SDTMIG-MD",
            Self::SendIg => "SENDIG",
            Self::SendIgAr => "SENDIG-AR",
            Self::SendIgDart => "SENDIG-DART",
            Self::SendIgGenetox => "SENDIG-GENETOX",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideStatus {
    Draft,
    Final,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    Submission,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    AnnotatedCrf,
    Supplemental,
    Other,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Study {
    pub origin: Origin,
    pub id: Field<String>,
    pub name: Field<String>,
    pub description: Field<String>,
    pub protocol_name: Field<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataVersion {
    pub origin: Origin,
    pub id: Field<String>,
    pub name: Field<String>,
    pub description: Option<Field<String>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImplementationGuide {
    pub origin: Origin,
    pub name: Field<GuideName>,
    pub version: Field<String>,
    pub status: Field<GuideStatus>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentReference {
    pub node: usize,
    pub id: Field<String>,
    pub kind: Field<DocumentKind>,
    pub link: Field<String>,
    pub title: Field<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub family: Submission,
    pub origin: Origin,
    pub standard: ImplementationGuide,
    pub specs: Field<Vec<Field<String>>>,
    pub define: Field<String>,
    pub documents: Field<Vec<DocumentReference>>,
    pub dataset_json: Field<bool>,
    pub context: Field<Context>,
    pub xml_lang: Field<String>,
    pub file_oid: Option<Field<String>>,
    pub metadata_version: Option<MetadataVersion>,
    pub originator: Option<Field<String>>,
    pub stylesheet: Option<Field<String>>,
    /// Authored metadata override; omission retains the installed-version default.
    pub source_system_version: Option<Field<String>>,
}

/// Borrowed metadata selection keeps the actual generator version separately.
/// This performs no copying, rendering or IO, and grants no Save capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceVersions<'a> {
    metadata: &'a str,
    installed: &'a str,
    override_origin: Option<Origin>,
}
impl SourceVersions<'_> {
    /// One selected value for both Define-XML and Dataset-JSON metadata.
    pub fn metadata(&self) -> &str {
        self.metadata
    }
    /// Qualification evidence always records the actual installed generator.
    pub fn installed(&self) -> &str {
        self.installed
    }
    pub fn override_origin(&self) -> Option<Origin> {
        self.override_origin
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionError {
    EmptyInstalledVersion,
    EmptyOverride,
}
impl Section {
    /// The calling package supplies its actual installed version. Core's own
    /// crate version is never a substitute for the Python or R generator.
    pub fn source_versions<'a>(
        &'a self,
        installed: &'a str,
    ) -> Result<SourceVersions<'a>, VersionError> {
        if installed.is_empty() {
            return Err(VersionError::EmptyInstalledVersion);
        }
        let metadata = self
            .source_system_version
            .as_ref()
            .map_or(installed, |field| field.value.as_str());
        if metadata.is_empty() {
            return Err(VersionError::EmptyOverride);
        }
        Ok(SourceVersions {
            metadata,
            installed,
            override_origin: self
                .source_system_version
                .as_ref()
                .map(|field| field.origin),
        })
    }
}
/// Owned metadata admitted only by the complete bounded decoder. It is not an
/// execution plan, semantic submission acceptance or a generator capability.
#[derive(Debug, PartialEq, Eq)]
pub struct Declarations {
    pub(super) root: usize,
    pub(super) study: Option<Study>,
    pub(super) sections: Vec<Section>,
}
impl Declarations {
    pub fn root(&self) -> usize {
        self.root
    }
    pub fn study(&self) -> Option<&Study> {
        self.study.as_ref()
    }
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }
}
