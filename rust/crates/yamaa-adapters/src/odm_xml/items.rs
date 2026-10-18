//! The clinical-item state machine over strict XML events, and the table
//! it fills. Context starts with each container and items are emitted when
//! their `ItemData` ends, so metadata applies only when it precedes an item.
use super::{
    xml::{Event, Tag, Xml},
    Error,
};
use crate::{arrow_table::physical_schema, ipc_table::TableError, public_table::PublicTable};
use arrow_array::{
    builder::{Int64Builder, StringBuilder},
    ArrayRef, RecordBatch,
};
use arrow_schema::SchemaRef;
use std::{collections::HashMap, io::Read, sync::Arc};
use yamaa_core::{
    limits::Limits,
    odm_items::{
        self, Definition, Document, Error as Refusal, Layout, Limit, ValueRefusal, COLUMNS,
        SOURCE_ORDINAL,
    },
};

const ODM_13: &str = "http://www.cdisc.org/ns/odm/v1.3";
const ODM_20: &str = "http://www.cdisc.org/ns/odm/v2.0";
/// The ODM 1.3 typed item elements (`ItemDataStarGroup`). Each carries a
/// clinical value the table has no column for, so it is refused, not dropped.
const TYPED_ITEM_DATA: [&str; 22] = [
    "ItemDataAny",
    "ItemDataString",
    "ItemDataInteger",
    "ItemDataFloat",
    "ItemDataDouble",
    "ItemDataDate",
    "ItemDataTime",
    "ItemDataDatetime",
    "ItemDataBoolean",
    "ItemDataHexBinary",
    "ItemDataBase64Binary",
    "ItemDataHexFloat",
    "ItemDataBase64Float",
    "ItemDataPartialDate",
    "ItemDataPartialTime",
    "ItemDataPartialDatetime",
    "ItemDataDurationDatetime",
    "ItemDataIntervalDatetime",
    "ItemDataIncompleteDatetime",
    "ItemDataIncompleteDate",
    "ItemDataIncompleteTime",
    "ItemDataURI",
];
/// Rows per record batch, as for engine outputs.
const BATCH_ROWS: usize = 65_536;
/// Arrow Utf8 arrays address their bytes with signed 32-bit offsets.
const OFFSET_LIMIT: usize = i32::MAX as usize;

/// Read every clinical item of one admitted XML stream.
pub(super) fn read<R: Read>(mut xml: Xml<R>) -> Result<PublicTable, Error> {
    let mut machine = Machine::default();
    loop {
        match xml.next()? {
            Event::Start(tag) => machine.start(tag)?,
            Event::End => machine.end()?,
            Event::Text(text) => machine.text(text),
            Event::Skip => {}
            Event::Eof => break,
        }
    }
    if machine.root.is_none() {
        return Err(Refusal::Document(Document::Root).into());
    }
    machine.sink.finish()
}

struct Root {
    namespace: String,
    version: String,
    file_oid: Option<String>,
}

struct Clinical {
    study: String,
    version: String,
}

struct Subject {
    key: String,
    study_subject_id: Option<String>,
    status: Option<String>,
}

struct StudyEvent {
    oid: String,
    repeat_key: Option<String>,
    name: Option<String>,
    start_date: Option<String>,
    status: Option<String>,
    workflow_status: Option<String>,
}

struct Form {
    oid: String,
    repeat_key: Option<String>,
    name: Option<String>,
    layout_oid: Option<String>,
    status: Option<String>,
    workflow_status: Option<String>,
    open_queries: Option<String>,
}

struct Group {
    oid: String,
    repeat_key: Option<String>,
    name: Option<String>,
    transaction_type: Option<String>,
}

/// What an `ItemData` inside `ClinicalData` held when it ends.
#[derive(Default)]
struct Item {
    oid: Option<String>,
    value: Option<String>,
    is_null: Option<String>,
    namespaced_value: bool,
    name: Option<Result<Option<String>, &'static str>>,
    value_children: usize,
    foreign_value: bool,
    value_elements: bool,
    value_text: String,
    text_outside: bool,
}

/// The open element stack, keeping what each end needs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Frame {
    Foreign,
    Root,
    Other,
    Study,
    MetaDataVersion,
    Container(Container),
    ItemData { active: bool },
    Value { capture: bool },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Container {
    Clinical,
    Subject,
    StudyEvent,
    Form,
    ItemGroup,
}

impl Container {
    fn name(self) -> &'static str {
        match self {
            Self::Clinical => "ClinicalData",
            Self::Subject => "SubjectData",
            Self::StudyEvent => "StudyEventData",
            Self::Form => "FormData",
            Self::ItemGroup => "ItemGroupData",
        }
    }
}

/// Metadata names keyed by exact (study OID, metadata version OID, OID).
type NameMap = HashMap<String, HashMap<String, HashMap<String, Option<String>>>>;

#[derive(Default)]
struct Names {
    maps: [NameMap; 4],
}

impl Names {
    fn index(kind: Definition) -> usize {
        match kind {
            Definition::Event => 0,
            Definition::Form => 1,
            Definition::Group => 2,
            Definition::Item => 3,
        }
    }

    fn add(
        &mut self,
        kind: Definition,
        study: &str,
        version: &str,
        oid: &str,
        name: Option<&str>,
    ) -> Result<(), Refusal> {
        let names = self.maps[Self::index(kind)]
            .entry(study.into())
            .or_default()
            .entry(version.into())
            .or_default();
        match names.get(oid) {
            Some(existing) if existing.as_deref() != name => Err(Refusal::MetadataConflict {
                kind,
                study: study.into(),
                version: version.into(),
                oid: oid.into(),
            }),
            Some(_) => Ok(()),
            None => {
                names.insert(oid.into(), name.map(Into::into));
                Ok(())
            }
        }
    }

    fn get(&self, kind: Definition, study: &str, version: &str, oid: &str) -> Option<&str> {
        self.maps[Self::index(kind)]
            .get(study)?
            .get(version)?
            .get(oid)?
            .as_deref()
    }
}

#[derive(Default)]
struct Machine {
    root: Option<Root>,
    names: Names,
    metadata_study: Option<String>,
    metadata_version: Option<String>,
    clinical: Option<Clinical>,
    subject: Option<Subject>,
    event: Option<StudyEvent>,
    form: Option<Form>,
    groups: Vec<Group>,
    frames: Vec<Frame>,
    items: Vec<Item>,
    item_ancestors: usize,
    ordinal: i64,
    sink: Sink,
}

/// The unprefixed attribute `attribute`, which must be non-empty.
fn required(tag: &Tag, element: &'static str, attribute: &'static str) -> Result<String, Refusal> {
    tag.plain(attribute)
        .filter(|value| !value.is_empty())
        .map(Into::into)
        .ok_or(Refusal::MissingAttribute { element, attribute })
}

fn optional(tag: &Tag, attribute: &str) -> Option<String> {
    tag.plain(attribute).map(Into::into)
}

/// The one attribute in any namespace with local name `name`.
fn extension(tag: &Tag, name: &'static str) -> Result<Option<String>, Refusal> {
    let mut matches = tag
        .attributes()
        .filter(|(namespace, local, _)| namespace.is_some() && *local == name)
        .map(|(_, _, value)| value);
    let first = matches.next();
    if matches.next().is_some() {
        return Err(Refusal::AmbiguousAttribute { name });
    }
    Ok(first.map(Into::into))
}

fn definition(local: &str) -> Option<(&'static str, Definition)> {
    Some(match local {
        "StudyEventDef" => ("StudyEventDef", Definition::Event),
        "FormDef" => ("FormDef", Definition::Form),
        "ItemGroupDef" => ("ItemGroupDef", Definition::Group),
        "ItemDef" => ("ItemDef", Definition::Item),
        _ => return None,
    })
}

/// Only XML's space, tab, CR and LF are layout.
fn is_layout(text: &str) -> bool {
    text.bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
}

impl Machine {
    fn start_root(&mut self, tag: &Tag) -> Result<(), Refusal> {
        if tag.local() != "ODM" {
            return Err(Refusal::Document(Document::Root));
        }
        let expected = match tag.namespace() {
            Some(ODM_13) => "1.3",
            Some(ODM_20) => "2.0",
            namespace => {
                return Err(Refusal::Document(Document::Namespace {
                    namespace: namespace.map(Into::into),
                }))
            }
        };
        let version = tag
            .plain("ODMVersion")
            .filter(|version| !version.is_empty())
            .unwrap_or(expected);
        if !odm_items::version_matches(version, expected) {
            return Err(Refusal::Document(Document::Version {
                version: version.into(),
            }));
        }
        self.root = Some(Root {
            namespace: tag.namespace().unwrap_or_default().into(),
            version: version.into(),
            file_oid: optional(tag, "FileOID"),
        });
        self.frames.push(Frame::Root);
        Ok(())
    }

    fn start(&mut self, tag: &Tag) -> Result<(), Error> {
        let Some(root) = &self.root else {
            return Ok(self.start_root(tag)?);
        };
        let core = tag.namespace() == Some(root.namespace.as_str());
        let local = tag.local();
        let mut value_child = None;
        match self.frames.last() {
            Some(Frame::ItemData { active: true }) if local == "Value" => {
                let item = self.items.last_mut().expect("active item record");
                if core {
                    item.value_children += 1;
                    value_child = Some(item.value_children == 1);
                } else {
                    item.foreign_value = true;
                }
            }
            Some(Frame::Value { capture: true }) => {
                self.items
                    .last_mut()
                    .expect("active item record")
                    .value_elements = true;
            }
            _ => {}
        }
        if !core {
            self.frames.push(Frame::Foreign);
            return Ok(());
        }
        let clinical = self.clinical.is_some();
        let frame = match local {
            "ODM" => return Err(Refusal::Document(Document::NestedOdm).into()),
            "Study" => {
                if self.metadata_study.is_some() {
                    return Err(Refusal::Layout(Layout::Nested("Study")).into());
                }
                if !clinical {
                    self.metadata_study = Some(required(tag, "Study", "OID")?);
                }
                Frame::Study
            }
            "MetaDataVersion" => {
                if self.metadata_version.is_some() {
                    return Err(Refusal::Layout(Layout::Nested("MetaDataVersion")).into());
                }
                if !clinical {
                    if self.metadata_study.is_none() {
                        return Err(Refusal::Layout(Layout::VersionOutsideStudy).into());
                    }
                    self.metadata_version = Some(required(tag, "MetaDataVersion", "OID")?);
                }
                Frame::MetaDataVersion
            }
            local if !clinical && definition(local).is_some() => {
                let (element, kind) = definition(local).expect("definition element");
                let (Some(study), Some(version)) = (&self.metadata_study, &self.metadata_version)
                else {
                    return Err(Refusal::Layout(Layout::DefinitionOutsideVersion(element)).into());
                };
                let oid = required(tag, element, "OID")?;
                self.names
                    .add(kind, study, version, &oid, tag.plain("Name"))?;
                Frame::Other
            }
            "ClinicalData" => {
                if clinical {
                    return Err(Refusal::Layout(Layout::Nested("ClinicalData")).into());
                }
                self.clinical = Some(Clinical {
                    study: required(tag, "ClinicalData", "StudyOID")?,
                    version: required(tag, "ClinicalData", "MetaDataVersionOID")?,
                });
                Frame::Container(Container::Clinical)
            }
            "SubjectData" if clinical => {
                if self.subject.is_some() {
                    return Err(Refusal::Layout(Layout::Nested("SubjectData")).into());
                }
                self.subject = Some(Subject {
                    key: required(tag, "SubjectData", "SubjectKey")?,
                    study_subject_id: extension(tag, "StudySubjectID")?,
                    status: extension(tag, "Status")?,
                });
                Frame::Container(Container::Subject)
            }
            "StudyEventData" if clinical => {
                if self.event.is_some() {
                    return Err(Refusal::Layout(Layout::Nested("StudyEventData")).into());
                }
                self.event = Some(StudyEvent {
                    oid: required(tag, "StudyEventData", "StudyEventOID")?,
                    repeat_key: optional(tag, "StudyEventRepeatKey"),
                    name: extension(tag, "EventName")?,
                    start_date: extension(tag, "StartDate")?,
                    status: extension(tag, "Status")?,
                    workflow_status: extension(tag, "WorkflowStatus")?,
                });
                Frame::Container(Container::StudyEvent)
            }
            "FormData" if clinical => {
                if self.form.is_some() {
                    return Err(Refusal::Layout(Layout::Nested("FormData")).into());
                }
                self.form = Some(Form {
                    oid: required(tag, "FormData", "FormOID")?,
                    repeat_key: optional(tag, "FormRepeatKey"),
                    name: extension(tag, "FormName")?,
                    layout_oid: extension(tag, "FormLayoutOID")?,
                    status: extension(tag, "Status")?,
                    workflow_status: extension(tag, "WorkflowStatus")?,
                    open_queries: extension(tag, "OpenQueries")?,
                });
                Frame::Container(Container::Form)
            }
            "ItemGroupData" if clinical => {
                self.groups.push(Group {
                    oid: required(tag, "ItemGroupData", "ItemGroupOID")?,
                    repeat_key: optional(tag, "ItemGroupRepeatKey"),
                    name: extension(tag, "ItemGroupName")?,
                    transaction_type: optional(tag, "TransactionType"),
                });
                Frame::Container(Container::ItemGroup)
            }
            "ItemData" => {
                self.item_ancestors += 1;
                if clinical {
                    self.items.push(Item {
                        oid: optional(tag, "ItemOID"),
                        value: optional(tag, "Value"),
                        is_null: optional(tag, "IsNull"),
                        namespaced_value: tag
                            .attributes()
                            .any(|(namespace, local, _)| namespace.is_some() && local == "Value"),
                        name: Some(extension(tag, "ItemName").map_err(|_| "ItemName")),
                        ..Item::default()
                    });
                }
                Frame::ItemData { active: clinical }
            }
            "Value" if value_child.is_some() => Frame::Value {
                capture: value_child == Some(true),
            },
            local
                if clinical
                    && tag.namespace() == Some(ODM_13)
                    && TYPED_ITEM_DATA.contains(&local) =>
            {
                return Err(Refusal::Layout(Layout::TypedItemData(local.into())).into());
            }
            _ => Frame::Other,
        };
        self.frames.push(frame);
        Ok(())
    }

    fn text(&mut self, text: &str) {
        match self.frames.last() {
            Some(Frame::ItemData { active: true }) if !is_layout(text) => {
                self.items
                    .last_mut()
                    .expect("active item record")
                    .text_outside = true;
            }
            Some(Frame::Value { capture: true }) => {
                self.items
                    .last_mut()
                    .expect("active item record")
                    .value_text
                    .push_str(text);
            }
            _ => {}
        }
    }

    fn end(&mut self) -> Result<(), Error> {
        match self.frames.pop() {
            Some(Frame::Study) if self.clinical.is_none() => self.metadata_study = None,
            Some(Frame::MetaDataVersion) if self.clinical.is_none() => self.metadata_version = None,
            Some(Frame::Container(container)) => {
                if self.item_ancestors > 0 {
                    return Err(Refusal::Layout(Layout::InsideItemData(container.name())).into());
                }
                match container {
                    Container::Clinical => self.clinical = None,
                    Container::Subject => self.subject = None,
                    Container::StudyEvent => self.event = None,
                    Container::Form => self.form = None,
                    Container::ItemGroup => {
                        self.groups.pop();
                    }
                }
            }
            Some(Frame::ItemData { active }) => {
                self.item_ancestors -= 1;
                if active {
                    let item = self.items.pop().expect("active item record");
                    if self.clinical.is_some() {
                        self.emit(item)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn emit(&mut self, item: Item) -> Result<(), Error> {
        if self.item_ancestors > 0 {
            return Err(Refusal::Layout(Layout::Nested("ItemData")).into());
        }
        let (Some(subject), Some(event)) = (&self.subject, &self.event) else {
            return Err(Refusal::Layout(Layout::ItemContext).into());
        };
        let oid = item
            .oid
            .filter(|oid| !oid.is_empty())
            .ok_or(Refusal::MissingAttribute {
                element: "ItemData",
                attribute: "ItemOID",
            })?;
        let refuse = |refusal| -> Error {
            Refusal::Value {
                item: oid.clone(),
                refusal,
            }
            .into()
        };
        if item.foreign_value {
            return Err(refuse(ValueRefusal::ForeignChild));
        }
        if item.namespaced_value {
            return Err(refuse(ValueRefusal::ForeignAttribute));
        }
        if item.text_outside {
            return Err(refuse(ValueRefusal::TextOutside));
        }
        if item.value.is_some() && item.value_children > 0 {
            return Err(refuse(ValueRefusal::AttributeAndChild));
        }
        if item.value_children > 1 {
            return Err(refuse(ValueRefusal::MultipleChildren));
        }
        if item.value_children == 1 && item.value_elements {
            return Err(refuse(ValueRefusal::ChildElements));
        }
        let value = match (item.value, item.value_children) {
            (Some(value), _) => Some(value),
            (None, 1) => Some(item.value_text),
            _ => None,
        };
        let is_null = odm_items::parse_is_null(item.is_null.as_deref())?;
        if is_null && value.is_some() {
            return Err(refuse(ValueRefusal::NullWithValue));
        }
        let clinical = self.clinical.as_ref().expect("active clinical data");
        let root = self.root.as_ref().expect("admitted root");
        let (form, group, form_kind) = match (&self.form, self.groups.as_slice()) {
            (Some(form), [group]) => (Projected::Form(form), group, Definition::Form),
            (None, [outer, inner]) => (Projected::Group(outer), inner, Definition::Group),
            _ => return Err(Refusal::Layout(Layout::ItemProjection).into()),
        };
        let item_name = item
            .name
            .unwrap_or(Ok(None))
            .map_err(|name| Refusal::AmbiguousAttribute { name })?;
        let (study, version) = (clinical.study.as_str(), clinical.version.as_str());
        let names = &self.names;
        let name = |source, kind, oid| named(names, source, kind, study, version, oid);
        let form_oid = form.oid();
        let values: [Option<&str>; SOURCE_ORDINAL] = [
            Some(&root.version),
            root.file_oid.as_deref(),
            Some(study),
            Some(version),
            Some(&subject.key),
            subject.study_subject_id.as_deref(),
            subject.status.as_deref(),
            Some(&event.oid),
            event.repeat_key.as_deref(),
            name(event.name.as_deref(), Definition::Event, &event.oid),
            event.start_date.as_deref(),
            event.status.as_deref(),
            event.workflow_status.as_deref(),
            Some(form_oid),
            form.repeat_key(),
            name(form.name(), form_kind, form_oid),
            form.layout_oid(),
            form.status(),
            form.workflow_status(),
            form.open_queries(),
            Some(&group.oid),
            group.repeat_key.as_deref(),
            name(group.name.as_deref(), Definition::Group, &group.oid),
            group.transaction_type.as_deref(),
            Some(&oid),
            name(item_name.as_deref(), Definition::Item, &oid),
            value.as_deref(),
            Some(odm_items::flag(value.is_some())),
            Some(odm_items::flag(is_null)),
        ];
        let ordinal = self.ordinal + 1;
        self.sink.push(values, ordinal)?;
        self.ordinal = ordinal;
        Ok(())
    }
}

/// A name from the data element itself, which wins even when empty, or else
/// from the exactly keyed metadata definition.
fn named<'a>(
    names: &'a Names,
    source: Option<&'a str>,
    kind: Definition,
    study: &str,
    version: &str,
    oid: &str,
) -> Option<&'a str> {
    source.or_else(|| names.get(kind, study, version, oid))
}

/// Form columns: a `FormData`, or the outer of two item groups.
enum Projected<'a> {
    Form(&'a Form),
    Group(&'a Group),
}

impl<'a> Projected<'a> {
    fn oid(&self) -> &'a str {
        match self {
            Self::Form(form) => &form.oid,
            Self::Group(group) => &group.oid,
        }
    }

    fn repeat_key(&self) -> Option<&'a str> {
        match self {
            Self::Form(form) => form.repeat_key.as_deref(),
            Self::Group(group) => group.repeat_key.as_deref(),
        }
    }

    fn name(&self) -> Option<&'a str> {
        match self {
            Self::Form(form) => form.name.as_deref(),
            Self::Group(group) => group.name.as_deref(),
        }
    }

    fn form(&self) -> Option<&'a Form> {
        match self {
            Self::Form(form) => Some(form),
            Self::Group(_) => None,
        }
    }

    fn layout_oid(&self) -> Option<&'a str> {
        self.form()?.layout_oid.as_deref()
    }

    fn status(&self) -> Option<&'a str> {
        self.form()?.status.as_deref()
    }

    fn workflow_status(&self) -> Option<&'a str> {
        self.form()?.workflow_status.as_deref()
    }

    fn open_queries(&self) -> Option<&'a str> {
        self.form()?.open_queries.as_deref()
    }
}

/// Rows appended in batches of the physical clinical-item schema.
struct Sink {
    schema: SchemaRef,
    text: Vec<StringBuilder>,
    ordinals: Int64Builder,
    bytes: [usize; SOURCE_ORDINAL],
    batch_rows: usize,
    rows: usize,
    batches: Vec<RecordBatch>,
}

impl Default for Sink {
    fn default() -> Self {
        Self {
            schema: physical_schema(&odm_items::schema()),
            text: (0..SOURCE_ORDINAL).map(|_| StringBuilder::new()).collect(),
            ordinals: Int64Builder::new(),
            bytes: [0; SOURCE_ORDINAL],
            batch_rows: 0,
            rows: 0,
            batches: Vec::new(),
        }
    }
}

impl Sink {
    fn push(&mut self, values: [Option<&str>; SOURCE_ORDINAL], ordinal: i64) -> Result<(), Error> {
        if self.rows >= Limits::TABLE_ROWS {
            return Err(Refusal::Limit(Limit::Rows(Limits::TABLE_ROWS)).into());
        }
        for (column, value) in values.iter().enumerate() {
            let length = value.map_or(0, str::len);
            if length > OFFSET_LIMIT {
                return Err(Refusal::Limit(Limit::TextColumn {
                    column: COLUMNS[column],
                    limit: OFFSET_LIMIT,
                })
                .into());
            }
            if self.bytes[column] + length > OFFSET_LIMIT {
                self.flush()?;
                break;
            }
        }
        for (column, value) in values.into_iter().enumerate() {
            self.bytes[column] += value.map_or(0, str::len);
            self.text[column].append_option(value);
        }
        self.ordinals.append_value(ordinal);
        self.batch_rows += 1;
        self.rows += 1;
        if self.batch_rows == BATCH_ROWS {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Error> {
        let mut arrays = self
            .text
            .iter_mut()
            .map(|builder| Arc::new(builder.finish()) as ArrayRef)
            .collect::<Vec<_>>();
        arrays.push(Arc::new(self.ordinals.finish()));
        let batch = RecordBatch::try_new(self.schema.clone(), arrays)
            .map_err(|_| Error::Table(TableError::Internal))?;
        self.batches.push(batch);
        self.bytes = [0; SOURCE_ORDINAL];
        self.batch_rows = 0;
        Ok(())
    }

    fn finish(mut self) -> Result<PublicTable, Error> {
        if self.batch_rows > 0 || self.batches.is_empty() {
            self.flush()?;
        }
        PublicTable::from_batches(self.schema, self.batches).map_err(Error::Table)
    }
}
