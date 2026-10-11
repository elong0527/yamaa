//! Deterministic UTF-8 Define-XML for the shared admitted column metadata profile.
use crate::{
    project_terminology as ct, specification_submission_document as s,
    submission_composition::{self as model, Finding, Study},
    submission_document as e,
    value::Value,
};
use alloc::{
    collections::BTreeSet,
    format,
    string::{String, ToString},
    vec::Vec,
};

const MAX_BYTES: usize = 67_108_864;
struct Element {
    name: &'static str,
    attributes: Vec<(&'static str, String)>,
    children: Vec<Element>,
    text: Option<String>,
    path: String,
}
impl Element {
    fn new(name: &'static str, path: &str) -> Self {
        Self {
            name,
            attributes: Vec::new(),
            children: Vec::new(),
            text: None,
            path: path.into(),
        }
    }
    fn attr(mut self, name: &'static str, value: impl ToString) -> Self {
        self.attributes.push((name, value.to_string()));
        self
    }
    fn optional(mut self, name: &'static str, value: Option<impl ToString>) -> Self {
        if let Some(value) = value {
            self.attributes.push((name, value.to_string()));
        }
        self
    }
    fn push(&mut self, child: Self) {
        self.children.push(child);
    }
    fn translated(&mut self, name: &'static str, text: &str, language: &str) {
        let mut wrapper = Self::new(name, &self.path);
        let mut translated = Self::new("TranslatedText", &self.path).attr("xml:lang", language);
        translated.text = Some(text.into());
        wrapper.push(translated);
        self.push(wrapper);
    }
    fn refs(&mut self, refs: &[s::DocumentReference]) {
        for reference in refs {
            let (id, pages) = match reference {
                s::DocumentReference::Plain(field) => (&field.value, None),
                s::DocumentReference::Class(class) => (&class.document.value, class.pages.as_ref()),
            };
            let mut node =
                Self::new("def:DocumentRef", &self.path).attr("leafID", format!("LF.{id}"));
            if let Some(pages) = pages {
                node.push(
                    Self::new("def:PDFPageRef", &self.path)
                        .attr("PageRefs", &pages.refs.value)
                        .attr("Type", pages.kind.value.name())
                        .optional("Title", pages.title.as_ref().map(|t| &t.value)),
                );
            }
            self.push(node);
        }
    }
    fn leaf(&mut self, id: &str, href: &str, title: &str) {
        let mut node = Self::new("def:leaf", &self.path)
            .attr("ID", format!("LF.{id}"))
            .attr("xlink:href", href);
        let mut child = Self::new("def:title", &self.path);
        child.text = Some(title.into());
        node.push(child);
        self.push(node);
    }
    fn write(&self, level: usize, out: &mut String) -> Result<(), Finding> {
        let append = |out: &mut String, text: &str| -> Result<(), Finding> {
            if out
                .len()
                .checked_add(text.len())
                .is_none_or(|n| n > MAX_BYTES)
            {
                return Err(Finding::cause(
                    crate::diagnostic::ConditionCode::ApplicationRejected,
                    &self.path,
                    "render_byte_limit",
                ));
            }
            out.push_str(text);
            Ok(())
        };
        let indent = "  ".repeat(level);
        append(out, &format!("{indent}<{}", self.name))?;
        for (name, value) in &self.attributes {
            append(
                out,
                &format!(" {name}=\"{}\"", escape(value, &self.path, true)?),
            )?;
        }
        if let Some(text) = &self.text {
            append(
                out,
                &format!(">{}</{}>\n", escape(text, &self.path, false)?, self.name),
            )?;
        } else if self.children.is_empty() {
            append(out, "/>\n")?;
        } else {
            append(out, ">\n")?;
            for child in &self.children {
                child.write(level + 1, out)?;
            }
            append(out, &format!("{indent}</{}>\n", self.name))?;
        }
        Ok(())
    }
    fn validate(
        &self,
        identities: &mut BTreeSet<String>,
        leaves: &mut BTreeSet<String>,
        references: &mut Vec<String>,
        oid_references: &mut Vec<String>,
    ) -> Result<(), Finding> {
        let invalid = |detail| Finding::new("value_not_permitted", "REQ-1026", &self.path, detail);
        for (name, value) in &self.attributes {
            match *name {
                "Name" | "Role" if value.is_empty() => {
                    return Err(invalid("schema_violation: nonempty name"))
                }
                "OID" | "ID" => {
                    if !identities.insert(value.clone()) {
                        return Err(Finding::new(
                            "duplicate_define_identifier",
                            "REQ-1017",
                            &self.path,
                            value,
                        ));
                    }
                    if *name == "ID" {
                        if !ncname(value) {
                            return Err(invalid("schema_violation: leaf ID NCName"));
                        }
                        leaves.insert(value.clone());
                    }
                }
                "leafID" => references.push(value.clone()),
                "def:ArchiveLocationID" => references.push(value.clone()),
                "ItemOID" | "def:ItemOID" | "CodeListOID" | "MethodOID" | "def:StandardOID"
                | "def:CommentOID" | "ValueListOID" | "WhereClauseOID" => {
                    oid_references.push(value.clone())
                }
                "xml:lang" if !language(value) => {
                    return Err(invalid("schema_violation: xml:lang language"))
                }
                _ => {}
            }
        }
        for child in &self.children {
            child.validate(identities, leaves, references, oid_references)?;
        }
        Ok(())
    }
}
fn language(value: &str) -> bool {
    let mut parts = value.split('-');
    parts
        .next()
        .is_some_and(|p| (1..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphabetic()))
        && parts.all(|p| (1..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()))
}
fn ncname(value: &str) -> bool {
    // XML 1.0 fifth edition NameStartChar/NameChar, excluding namespace colon.
    let start = |c| matches!(c, 'A'..='Z' | 'a'..='z' | '_' | '\u{c0}'..='\u{d6}' | '\u{d8}'..='\u{f6}' | '\u{f8}'..='\u{2ff}' | '\u{370}'..='\u{37d}' | '\u{37f}'..='\u{1fff}' | '\u{200c}'..='\u{200d}' | '\u{2070}'..='\u{218f}' | '\u{2c00}'..='\u{2fef}' | '\u{3001}'..='\u{d7ff}' | '\u{f900}'..='\u{fdcf}' | '\u{fdf0}'..='\u{fffd}' | '\u{10000}'..='\u{effff}');
    let mut chars = value.chars();
    chars.next().is_some_and(start) && chars.all(|c| start(c) || matches!(c, '-' | '.' | '0'..='9' | '\u{b7}' | '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}'))
}
fn escape(value: &str, path: &str, attribute: bool) -> Result<String, Finding> {
    if value
        .chars()
        .any(|c| c == '\r' || c == '\t' || attribute && c == '\n')
    {
        return Err(Finding::new(
            "untransportable_text",
            "REQ-1025",
            path,
            "forbidden XML whitespace",
        ));
    }
    if value
        .chars()
        .any(|c| c < '\u{20}' && c != '\n' || c == '\u{fffe}' || c == '\u{ffff}')
    {
        return Err(Finding::new(
            "value_not_permitted",
            "REQ-1026",
            path,
            "schema_violation: forbidden XML character",
        ));
    }
    let mut out = value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    if attribute {
        out = out.replace('"', "&quot;");
    }
    Ok(out)
}
fn scalar(value: &Value) -> String {
    match value {
        Value::Str(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => crate::conversion::float_text(*f),
        _ => String::new(),
    }
}
fn yes(value: bool) -> &'static str {
    if value {
        "Yes"
    } else {
        "No"
    }
}
fn comment(node: &mut Element, comment: &s::Comment, language: &str) {
    match comment {
        s::Comment::Plain(text) => node.translated("Description", &text.value, language),
        s::Comment::Class(class) => {
            node.translated("Description", &class.text.value, language);
            node.refs(&class.documents.value);
        }
    }
}
fn item_definition(column: &model::Column, identity: &str, path: &str, language: &str) -> Element {
    let name = &column.declaration.name.value;
    let meta = column
        .declaration
        .submission
        .as_ref()
        .expect("composed column");
    let origin = meta.origin_metadata.as_ref().expect("composed origin");
    let mut item = Element::new("ItemDef", path)
        .attr("OID", format!("IT.{identity}"))
        .attr("Name", name)
        .attr("DataType", column.data_type.name())
        .optional("Length", column.length.as_ref())
        .optional(
            "SignificantDigits",
            meta.significant_digits.as_ref().map(|v| &v.value),
        )
        .optional("SASFieldName", model::sas_name(name).then_some(name))
        .optional(
            "def:DisplayFormat",
            meta.display_format.as_ref().map(|v| &v.value),
        )
        .optional(
            "def:CommentOID",
            meta.comment.as_ref().map(|_| format!("COM.{identity}")),
        );
    item.translated(
        "Description",
        &column
            .declaration
            .label
            .as_ref()
            .expect("composed label")
            .value,
        language,
    );
    if let Some(list) = &meta.codelist {
        item.push(
            Element::new("CodeListRef", &item.path)
                .attr("CodeListOID", format!("CL.{}", list.value)),
        );
    }
    let mut node = Element::new("def:Origin", &item.path)
        .attr("Type", origin.kind.value.name())
        .optional("Source", column.source.map(|v| v.name()));
    if let Some(description) = &origin.description {
        node.translated("Description", &description.value, language);
    }
    node.refs(&column.documents);
    item.push(node);
    item
}
fn variants(dataset: &model::Dataset) -> impl Iterator<Item = (&model::Column, String)> {
    dataset.columns.iter().flat_map(move |column| {
        let identity = format!("{}.{}", dataset.id, column.declaration.name.value);
        core::iter::once((column, identity.clone())).chain(
            column
                .values
                .iter()
                .map(move |value| (&value.column, format!("{identity}.{}", value.test_code))),
        )
    })
}

/// The writer's closed element construction fixes official schema order and
/// references. Composition admits required attributes, numeric ranges, types,
/// keys and cross-references; character admission happens before bytes escape.
pub fn render(study: &Study) -> Result<Vec<u8>, Finding> {
    validate_profile(study)?;
    let section = &study.section;
    let language = &section.xml_lang.value;
    let mut odm = Element::new("ODM", "study")
        .attr("xmlns", "http://www.cdisc.org/ns/odm/v1.3")
        .attr("xmlns:def", "http://www.cdisc.org/ns/def/v2.1")
        .attr("xmlns:xlink", "http://www.w3.org/1999/xlink")
        .attr("ODMVersion", "1.3.2")
        .attr("FileType", "Snapshot")
        .attr("FileOID", &study.file_oid)
        .attr("CreationDateTime", &study.created)
        .optional("Originator", section.originator.as_ref().map(|v| &v.value))
        .attr("SourceSystem", "yamaa")
        .attr("SourceSystemVersion", &study.source_version)
        .attr(
            "def:Context",
            match section.context.value {
                e::Context::Submission => "Submission",
                e::Context::Other => "Other",
            },
        );
    let mut root =
        Element::new("Study", "study").attr("OID", format!("STDY.{}", study.study.id.value));
    let mut globals = Element::new("GlobalVariables", "study");
    for (name, value) in [
        ("StudyName", &study.study.name.value),
        ("StudyDescription", &study.study.description.value),
        ("ProtocolName", &study.study.protocol_name.value),
    ] {
        let mut node = Element::new(name, "study");
        node.text = Some(value.clone());
        globals.push(node);
    }
    root.push(globals);
    let mut metadata = Element::new("MetaDataVersion", "metadata_version")
        .attr("OID", format!("MDV.{}", study.metadata_id))
        .attr("Name", &study.metadata_name)
        .optional("Description", study.metadata_description.as_ref())
        .attr("def:DefineVersion", "2.1.11");
    let mut standards = Element::new("def:Standards", "standards");
    for standard in &study.standards {
        standards.push(
            Element::new("def:Standard", "standards")
                .attr("OID", format!("STD.{}", standard.id))
                .attr("Name", &standard.name)
                .attr("Type", standard.kind)
                .optional("PublishingSet", standard.publishing_set.as_ref())
                .attr("Version", &standard.version)
                .attr("Status", standard.status),
        );
    }
    metadata.push(standards);
    for (kind, name) in [
        (e::DocumentKind::AnnotatedCrf, "def:AnnotatedCRF"),
        (e::DocumentKind::Supplemental, "def:SupplementalDoc"),
    ] {
        let mut parent = Element::new(name, "documents");
        for document in &section.documents.value {
            if document.kind.value == kind {
                parent.push(
                    Element::new("def:DocumentRef", "documents")
                        .attr("leafID", format!("LF.{}", document.id.value)),
                );
            }
        }
        if !parent.children.is_empty() {
            metadata.push(parent);
        }
    }
    for dataset in &study.datasets {
        for column in &dataset.columns {
            if column.values.is_empty() {
                continue;
            }
            let identity = format!("{}.{}", dataset.id, column.declaration.name.value);
            let mut list = Element::new("def:ValueListDef", &format!("datasets.{identity}.values"))
                .attr("OID", format!("VL.{identity}"));
            for (index, value) in column.values.iter().enumerate() {
                let identity = format!("{identity}.{}", value.test_code);
                let meta = value
                    .column
                    .declaration
                    .submission
                    .as_ref()
                    .expect("composed value");
                let mut reference = Element::new("ItemRef", &value.path)
                    .attr("ItemOID", format!("IT.{identity}"))
                    .attr("OrderNumber", index + 1)
                    .attr("Mandatory", yes(value.column.mandatory))
                    .optional("Role", meta.role.as_ref().map(|v| &v.value))
                    .optional(
                        "MethodOID",
                        meta.method.as_ref().map(|_| format!("MT.{identity}")),
                    );
                reference.push(
                    Element::new("def:WhereClauseRef", &value.path)
                        .attr("WhereClauseOID", format!("WC.{identity}")),
                );
                list.push(reference);
            }
            metadata.push(list);
        }
    }
    for dataset in &study.datasets {
        for column in &dataset.columns {
            for value in &column.values {
                let identity = format!(
                    "{}.{}.{}",
                    dataset.id, column.declaration.name.value, value.test_code
                );
                let discriminator = &dataset.columns
                    [dataset.discriminator.expect("admitted discriminator")]
                .declaration
                .name
                .value;
                let mut clause = Element::new("def:WhereClauseDef", &value.path)
                    .attr("OID", format!("WC.{identity}"));
                let mut check = Element::new("RangeCheck", &value.path)
                    .attr("Comparator", "EQ")
                    .attr("SoftHard", "Soft")
                    .attr("def:ItemOID", format!("IT.{}.{discriminator}", dataset.id));
                let mut code = Element::new("CheckValue", &value.path);
                code.text = Some(value.test_code.clone());
                check.push(code);
                clause.push(check);
                metadata.push(clause);
            }
        }
    }
    for dataset in &study.datasets {
        let id = &dataset.id;
        let sub = &dataset.metadata;
        let mut group = Element::new("ItemGroupDef", &format!("datasets.{id}.submission"))
            .attr("OID", format!("IG.{id}"))
            .attr("Name", id)
            .optional(
                "Domain",
                if section.family == crate::project_environment::Submission::Adam {
                    None
                } else {
                    Some(&dataset.domain)
                },
            )
            .attr(
                "Purpose",
                if section.family == crate::project_environment::Submission::Adam {
                    "Analysis"
                } else {
                    "Tabulation"
                },
            )
            .optional("SASDatasetName", model::sas_name(id).then_some(id))
            .attr("Repeating", yes(sub.repeating.value))
            .attr("IsReferenceData", yes(sub.reference_data.value))
            .attr("def:Structure", &sub.structure.value)
            .attr("def:ArchiveLocationID", format!("LF.{id}"))
            .attr(
                "def:StandardOID",
                format!("STD.{}", section.standard.name.value.name()),
            )
            .optional(
                "def:CommentOID",
                sub.comment.as_ref().map(|_| format!("COM.{id}")),
            )
            .optional("def:HasNoData", sub.has_no_data.value.then_some("Yes"));
        group.translated("Description", &sub.label.value, language);
        for (index, column) in dataset.columns.iter().enumerate() {
            let name = &column.declaration.name.value;
            let meta = column
                .declaration
                .submission
                .as_ref()
                .expect("composed column");
            group.push(
                Element::new("ItemRef", &group.path)
                    .attr("ItemOID", format!("IT.{id}.{name}"))
                    .attr("OrderNumber", index + 1)
                    .attr("Mandatory", yes(column.mandatory))
                    .optional("KeySequence", column.key_sequence)
                    .optional("Role", meta.role.as_ref().map(|v| &v.value))
                    .optional(
                        "MethodOID",
                        meta.method.as_ref().map(|_| format!("MT.{id}.{name}")),
                    ),
            );
        }
        let mut class = Element::new("def:Class", &group.path).attr("Name", sub.class.value.name());
        if let Some(subclass) = &sub.subclass {
            class.push(
                Element::new("def:SubClass", &group.path).attr("Name", subclass.value.name()),
            );
        }
        group.push(class);
        group.leaf(
            id,
            &dataset.href,
            dataset.href.rsplit('/').next().unwrap_or(&dataset.href),
        );
        metadata.push(group);
    }
    for dataset in &study.datasets {
        for (column, identity) in variants(dataset) {
            let mut item = item_definition(
                column,
                &identity,
                &format!("datasets.{identity}.submission"),
                language,
            );
            if !column.values.is_empty() {
                item.push(
                    Element::new("def:ValueListRef", &item.path)
                        .attr("ValueListOID", format!("VL.{identity}")),
                );
            }
            metadata.push(item);
        }
    }
    for list in &study.codelists {
        let c = &list.declaration;
        let mut code = Element::new("CodeList", &format!("codelists.{}", c.id))
            .attr("OID", format!("CL.{}", c.id))
            .attr("Name", &c.name)
            .attr(
                "DataType",
                match c.data_type {
                    ct::DataType::Text => "text",
                    ct::DataType::Integer => "integer",
                    ct::DataType::Float => "float",
                },
            )
            .optional(
                "def:StandardOID",
                list.standard.as_ref().map(|id| format!("STD.{id}")),
            )
            .optional(
                "def:IsNonStandard",
                (list.standard.is_none() && c.items.is_some()).then_some("Yes"),
            )
            .optional("SASFormatName", c.format_name.as_ref());
        for item in c.items.as_deref().unwrap_or(&[]) {
            let mut node = Element::new(
                if item.decode.is_some() {
                    "CodeListItem"
                } else {
                    "EnumeratedItem"
                },
                &code.path,
            )
            .attr("CodedValue", scalar(&item.value))
            .optional("Rank", item.rank)
            .optional("def:ExtendedValue", item.extended.then_some("Yes"));
            if let Some(decode) = &item.decode {
                node.translated("Decode", decode, language);
            }
            if let Some(alias) = &item.alias {
                node.push(
                    Element::new("Alias", &code.path)
                        .attr("Context", "nci:ExtCodeID")
                        .attr("Name", alias),
                );
            }
            code.push(node);
        }
        if let Some(external) = &c.external {
            code.push(
                Element::new("ExternalCodeList", &code.path)
                    .attr("Dictionary", &external.dictionary)
                    .attr("Version", &external.version)
                    .optional("href", external.href.as_ref()),
            );
        }
        if let Some(alias) = &c.alias {
            code.push(
                Element::new("Alias", &code.path)
                    .attr("Context", "nci:ExtCodeID")
                    .attr("Name", alias),
            );
        }
        metadata.push(code);
    }
    for dataset in &study.datasets {
        for (column, name) in variants(dataset) {
            let meta = column
                .declaration
                .submission
                .as_ref()
                .expect("composed column");
            let Some(method) = &meta.method else { continue };
            let mut node = Element::new("MethodDef", &format!("datasets.{name}.submission.method"))
                .attr("OID", format!("MT.{name}"));
            match method {
                s::Method::Plain(description) => {
                    node = node.attr("Name", &name).attr("Type", "Computation");
                    node.translated("Description", &description.value, language);
                }
                s::Method::Class(class) => {
                    node = node
                        .attr("Name", class.name.as_ref().map_or(&name, |v| &v.value))
                        .attr("Type", class.kind.value.name());
                    node.translated("Description", &class.description.value, language);
                    if let Some(expression) = &class.expression {
                        let mut formal = Element::new("FormalExpression", &node.path)
                            .attr("Context", &expression.context.value);
                        formal.text = Some(expression.code.value.clone());
                        node.push(formal);
                    }
                    node.refs(&class.documents.value);
                }
            }
            metadata.push(node);
        }
    }
    for dataset in &study.datasets {
        let mut comments: Vec<(String, &s::Comment)> = Vec::new();
        if let Some(c) = &dataset.metadata.comment {
            comments.push((dataset.id.clone(), c));
        }
        for (column, name) in variants(dataset) {
            if let Some(c) = &column
                .declaration
                .submission
                .as_ref()
                .expect("composed column")
                .comment
            {
                comments.push((name, c));
            }
        }
        for (name, c) in comments {
            let mut node = Element::new("def:CommentDef", &format!("datasets.{name}.comment"))
                .attr("OID", format!("COM.{name}"));
            comment(&mut node, c, language);
            metadata.push(node);
        }
    }
    for document in &section.documents.value {
        metadata.leaf(
            &document.id.value,
            &document.link.value,
            &document.title.value,
        );
    }
    root.push(metadata);
    odm.push(root);
    let mut identities = BTreeSet::new();
    let mut leaves = BTreeSet::new();
    let mut references = Vec::new();
    let mut oid_references = Vec::new();
    odm.validate(
        &mut identities,
        &mut leaves,
        &mut references,
        &mut oid_references,
    )?;
    if oid_references
        .iter()
        .any(|reference| !identities.contains(reference))
    {
        return Err(Finding::new(
            "value_not_permitted",
            "REQ-1026",
            "metadata_version",
            "schema_violation: unresolved generated metadata reference",
        ));
    }
    if references
        .iter()
        .any(|reference| !leaves.contains(reference))
    {
        return Err(Finding::new(
            "unknown_document",
            "REQ-1016",
            "documents",
            "unresolved generated leaf reference",
        ));
    }
    let mut out = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n".to_string();
    if let Some(stylesheet) = &section.stylesheet {
        out.push_str(&format!(
            "<?xml-stylesheet type=\"text/xsl\" href=\"{}\"?>\n",
            escape(&stylesheet.value, "stylesheet", true)?
        ));
    }
    odm.write(0, &mut out)?;
    Ok(out.into_bytes())
}
fn validate_profile(study: &Study) -> Result<(), Finding> {
    let invalid = |path: &str, detail: &str| {
        Finding::new(
            "value_not_permitted",
            "REQ-1026",
            path,
            &format!("schema_violation: {detail}"),
        )
    };
    if study.created.len() != 19 || study.created.parse::<crate::temporal::DateTime>().is_err() {
        return Err(invalid("created", "CreationDateTime"));
    }
    // Complete scalar facets for this closed generated element profile. The
    // immutable writer fixes element order/cardinality and generated references;
    // independent installed tests validate bytes with the unchanged official XSD.
    for standard in &study.standards {
        if !matches!(
            standard.name.as_str(),
            "ADaMIG"
                | "ADaMIG-MD"
                | "BIMO"
                | "CDISC/NCI"
                | "SDTMIG"
                | "SDTMIG-AP"
                | "SDTMIG-MD"
                | "SENDIG"
                | "SENDIG-AR"
                | "SENDIG-DART"
                | "SENDIG-GENETOX"
        ) {
            return Err(invalid("standards", "StandardName enumeration"));
        }
        if standard.publishing_set.as_ref().is_some_and(|set| {
            !matches!(
                set.as_str(),
                "ADaM" | "CDASH" | "DEFINE-XML" | "SDTM" | "SEND"
            )
        }) {
            return Err(invalid("standards", "StandardPublishingSet enumeration"));
        }
    }
    for list in &study.codelists {
        let declaration = &list.declaration;
        if declaration.items.as_ref().is_some_and(Vec::is_empty) {
            return Err(invalid(
                &format!("codelists.{}", declaration.id),
                "CodeList requires an item or dictionary",
            ));
        }
        if let Some(format) = &declaration.format_name {
            let mut bytes = format.bytes();
            if format.len() > 8
                || !bytes
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || b"_$".contains(&c))
                || !bytes.all(|c| c.is_ascii_alphanumeric() || b"_.".contains(&c))
            {
                return Err(invalid(
                    &format!("codelists.{}.format_name", declaration.id),
                    "SASFormatName facet",
                ));
            }
        }
    }
    Ok(())
}
