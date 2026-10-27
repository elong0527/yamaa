//! Deterministic UTF-8 Define-XML for the shared admitted column metadata profile.
//!
//! yamaa-core owns the composed metadata model (`submission_composition`) and
//! the findings this writer raises (`define_diagnostics`); this writer owns the
//! element tree, its schema checks, escaping and the bytes.
use std::collections::BTreeSet;
use yamaa_core::{
    application_issue::{Fault, Step},
    ast,
    cdisc_vocabulary::{OriginType, PublishingSet, StandardName},
    define_diagnostics::{xml_invalid, xml_whitespace},
    diagnostic::ConditionCode,
    limits::Limits,
    project_environment::Submission,
    specification_submission_document as s,
    submission_composition::{self as model, Finding, Study},
    value::Value,
};

struct Element {
    name: &'static str,
    attributes: Vec<(&'static str, String)>,
    children: Vec<Element>,
    text: Option<String>,
    text_path: Option<String>,
    path: String,
}
impl Element {
    fn new(name: &'static str, path: &str) -> Self {
        Self {
            name,
            attributes: Vec::new(),
            children: Vec::new(),
            text: None,
            text_path: None,
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
        self.translated_at(name, text, language, None);
    }
    fn translated_at(
        &mut self,
        name: &'static str,
        text: &str,
        language: &str,
        text_path: Option<&str>,
    ) {
        let mut wrapper = Self::new(name, &self.path);
        let mut translated = Self::new("TranslatedText", &self.path).attr("xml:lang", language);
        translated.text = Some(text.into());
        translated.text_path = text_path.map(String::from);
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
    fn leaf(&mut self, id: &str, href: &str, title: &str, title_path: Option<&str>) {
        let mut node = Self::new("def:leaf", &self.path)
            .attr("ID", format!("LF.{id}"))
            .attr("xlink:href", href);
        let mut child = Self::new("def:title", &self.path);
        child.text = Some(title.into());
        child.text_path = title_path.map(String::from);
        node.push(child);
        self.push(node);
    }
    fn write(&self, level: usize, out: &mut String) -> Result<(), Finding> {
        let append = |out: &mut String, text: &str| -> Result<(), Finding> {
            Limits::output_extension(out.len(), text.len()).map_err(|bound| {
                Finding::limit(&self.path, Step::Define, Fault::OutputLimit, bound)
                    .with_detail("render_byte_limit")
            })?;
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
                &format!(
                    ">{}</{}>\n",
                    escape(text, self.text_path.as_deref().unwrap_or(&self.path), false)?,
                    self.name
                ),
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
        // Each refusal names the attribute value that breaks the schema.
        let invalid = |facet, value: &str| xml_invalid(&self.path, facet, text(value));
        for (name, value) in &self.attributes {
            match *name {
                "Name" | "Role" if value.is_empty() => return Err(invalid("nonempty name", value)),
                "OID" | "ID" => {
                    if !identities.insert(value.clone()) {
                        return Err(Finding::cause(
                            ConditionCode::SubmissionDuplicateIdentifier,
                            &self.path,
                            value,
                        )
                        .with("identifier", value.as_str()));
                    }
                    if *name == "ID" {
                        if !ncname(value) {
                            return Err(invalid("leaf ID NCName", value));
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
                "xml:lang" if !language(value) => return Err(invalid("xml:lang language", value)),
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
/// A finding's text value.
fn text(value: &str) -> yamaa_core::json::Value {
    yamaa_core::json::Value::String(value.into())
}
fn escape(value: &str, path: &str, attribute: bool) -> Result<String, Finding> {
    if let Some(character) = value
        .chars()
        .find(|&c| c == '\r' || c == '\t' || attribute && c == '\n')
    {
        return Err(xml_whitespace(path, character));
    }
    // The forbidden character, named by its code point.
    if let Some(character) = value
        .chars()
        .find(|&c| c < '\u{20}' && c != '\n' || c == '\u{fffe}' || c == '\u{ffff}')
    {
        return Err(xml_invalid(
            path,
            "forbidden XML character",
            text(&format!("U+{:04X}", u32::from(character))),
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
        Value::Float(f) => yamaa_core::conversion::float_text(*f),
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
fn item_definition(
    column: &model::Column,
    identity: &str,
    path: &str,
    label_path: &str,
    language: &str,
    version: ast::project::DefineVersion,
    family: Submission,
) -> Element {
    let modern = matches!(version, ast::project::DefineVersion::V2_1(_));
    let name = &column.declaration().name.value;
    let meta = column
        .declaration()
        .submission
        .as_ref()
        .expect("composed column");
    let mut item = Element::new("ItemDef", path)
        .attr("OID", format!("IT.{identity}"))
        .attr("Name", name)
        .attr("DataType", column.data_type().name())
        .optional("Length", column.length())
        .optional(
            "SignificantDigits",
            meta.significant_digits.as_ref().map(|v| &v.value),
        )
        .optional("SASFieldName", model::column_name(name).then_some(name))
        .optional("def:DisplayFormat", column.display_format())
        .optional(
            "def:CommentOID",
            meta.comment.as_ref().map(|_| format!("COM.{identity}")),
        );
    item.translated_at(
        "Description",
        &column
            .declaration()
            .label
            .as_ref()
            .expect("composed label")
            .value,
        language,
        Some(label_path),
    );
    if let Some(list) = &meta.codelist {
        item.push(
            Element::new("CodeListRef", &item.path)
                .attr("CodeListOID", format!("CL.{}", list.value)),
        );
    }
    // One `def:Origin` per declared origin, in declaration order (DEFINE-19).
    for origin in column.origins() {
        let declared = origin.declaration();
        let mut node = Element::new("def:Origin", &item.path)
            .attr(
                "Type",
                if modern {
                    declared.kind.value.name()
                } else {
                    legacy_origin(declared.kind.value, family).expect("validated legacy origin")
                },
            )
            .optional(
                "Source",
                origin.source().filter(|_| modern).map(|v| v.name()),
            );
        if let Some(description) = &declared.description {
            node.translated("Description", &description.value, language);
        }
        node.refs(origin.documents());
        item.push(node);
    }
    item
}
/// Every base and per-value definition of a dataset, in `ItemDef` order: each
/// column followed by its value-level entries, with the entry `id` closing
/// the per-value identity (DEFINE-9).
fn variants(dataset: &model::Dataset) -> impl Iterator<Item = (&model::Column, String)> {
    dataset.columns().iter().flat_map(move |column| {
        let identity = format!("{}.{}", dataset.id(), column.declaration().name.value);
        std::iter::once((column, identity.clone())).chain(
            column
                .values()
                .iter()
                .map(move |value| (value.column(), format!("{identity}.{}", value.id()))),
        )
    })
}

/// Append one value-list definition per column that declares values, in
/// dataset and column order.
fn push_value_lists(metadata: &mut Element, study: &Study) {
    for dataset in study.datasets() {
        for column in dataset.columns() {
            if column.values().is_empty() {
                continue;
            }
            let identity = format!("{}.{}", dataset.id(), column.declaration().name.value);
            let mut list = Element::new("def:ValueListDef", &format!("datasets.{identity}.values"))
                .attr("OID", format!("VL.{identity}"));
            for (index, value) in column.values().iter().enumerate() {
                let identity = format!("{identity}.{}", value.id());
                let meta = value
                    .column()
                    .declaration()
                    .submission
                    .as_ref()
                    .expect("composed value");
                let mut reference = Element::new("ItemRef", value.path())
                    .attr("ItemOID", format!("IT.{identity}"))
                    .attr("OrderNumber", index + 1)
                    .attr("Mandatory", yes(value.column().mandatory()))
                    .optional(
                        "KeySequence",
                        value
                            .column()
                            .key_sequence()
                            .filter(|_| meta.key_sequence.is_some()),
                    )
                    .optional("Role", meta.role.as_ref().map(|v| &v.value))
                    .optional(
                        "MethodOID",
                        meta.method.as_ref().map(|_| format!("MT.{identity}")),
                    );
                reference.push(
                    Element::new("def:WhereClauseRef", value.path())
                        .attr("WhereClauseOID", format!("WC.{identity}")),
                );
                list.push(reference);
            }
            metadata.push(list);
        }
    }
}

/// Append one where-clause definition per declared value, in dataset,
/// column and value order.
fn push_where_clauses(metadata: &mut Element, study: &Study) {
    for dataset in study.datasets() {
        for column in dataset.columns() {
            for value in column.values() {
                let identity = format!(
                    "{}.{}.{}",
                    dataset.id(),
                    column.declaration().name.value,
                    value.id()
                );
                let mut clause = Element::new("def:WhereClauseDef", value.path())
                    .attr("OID", format!("WC.{identity}"));
                // One range check per where column, in written order: one
                // value is an equality and several a membership, and an
                // exclusion is NE for one value and NOTIN for several
                // (DEFINE-27).
                for term in value.clause() {
                    let target = &dataset.columns()[term.column].declaration().name.value;
                    let comparator = match (term.excludes, term.values.len()) {
                        (false, 1) => "EQ",
                        (false, _) => "IN",
                        (true, 1) => "NE",
                        (true, _) => "NOTIN",
                    };
                    let mut check = Element::new("RangeCheck", value.path())
                        .attr("Comparator", comparator)
                        .attr("SoftHard", "Soft")
                        .attr("def:ItemOID", format!("IT.{}.{target}", dataset.id()));
                    for checked in &term.values {
                        let mut code = Element::new("CheckValue", value.path());
                        code.text = Some(checked.clone());
                        check.push(code);
                    }
                    clause.push(check);
                }
                metadata.push(clause);
            }
        }
    }
}

/// Append one item-group definition per dataset, in dataset order.
fn push_item_groups(metadata: &mut Element, study: &Study, language: &str) {
    let section = study.section();
    let modern = matches!(section.define_version, ast::project::DefineVersion::V2_1(_));
    for dataset in study.datasets() {
        let id = dataset.id();
        let sub = dataset.metadata();
        let mut group = Element::new("ItemGroupDef", &format!("datasets.{id}.submission"))
            .attr("OID", format!("IG.{id}"))
            .attr("Name", id)
            .optional(
                "Domain",
                if section.family == Submission::Adam {
                    None
                } else {
                    Some(dataset.domain())
                },
            )
            .attr(
                "Purpose",
                if section.family == Submission::Adam {
                    "Analysis"
                } else {
                    "Tabulation"
                },
            )
            .optional("SASDatasetName", model::dataset_name(id).then_some(id))
            .attr("Repeating", yes(sub.repeating.value))
            .attr("IsReferenceData", yes(sub.reference_data.value))
            .attr("def:Structure", &sub.structure.value)
            .optional("def:Class", (!modern).then_some(sub.class.value.name()))
            // A dataset with no data and no transport file has no leaf.
            .optional(
                "def:ArchiveLocationID",
                dataset.href().map(|_| format!("LF.{id}")),
            )
            // The guide the dataset selects, or the section's first (DEFINE-6).
            .optional(
                "def:StandardOID",
                modern.then(|| format!("STD.{}", dataset.standard())),
            )
            .optional(
                "def:CommentOID",
                sub.comment.as_ref().map(|_| format!("COM.{id}")),
            )
            .optional("def:HasNoData", sub.has_no_data.value.then_some("Yes"));
        group.translated("Description", &sub.label.value, language);
        for (index, column) in dataset.columns().iter().enumerate() {
            let name = &column.declaration().name.value;
            let meta = column
                .declaration()
                .submission
                .as_ref()
                .expect("composed column");
            group.push(
                Element::new("ItemRef", &group.path)
                    .attr("ItemOID", format!("IT.{id}.{name}"))
                    .attr("OrderNumber", index + 1)
                    .attr("Mandatory", yes(column.mandatory()))
                    .optional("KeySequence", column.key_sequence())
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
        if modern {
            group.push(class);
        }
        if let Some(href) = dataset.href() {
            group.leaf(id, href, href.rsplit('/').next().unwrap_or(href), None);
        }
        metadata.push(group);
    }
}

/// Append one code-list definition per bound codelist, in catalogue order.
fn push_codelists(metadata: &mut Element, study: &Study, language: &str) {
    let modern = matches!(
        study.section().define_version,
        ast::project::DefineVersion::V2_1(_)
    );
    for list in study.codelists() {
        let c = &list.declaration;
        let mut code = Element::new("CodeList", &format!("codelists.{}", c.id.name))
            .attr("OID", format!("CL.{}", c.id.name))
            .attr("Name", &c.name.text)
            .attr("DataType", c.data_type.name())
            .optional(
                "def:StandardOID",
                list.standard
                    .as_ref()
                    .filter(|_| modern)
                    .map(|id| format!("STD.{id}")),
            )
            .optional(
                "def:IsNonStandard",
                (modern && list.standard.is_none() && c.items.is_some()).then_some("Yes"),
            )
            .optional("SASFormatName", c.format_name.as_ref().map(|t| &t.text));
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
                node.translated("Decode", &decode.text, language);
            }
            if let Some(alias) = &item.alias {
                node.push(
                    Element::new("Alias", &code.path)
                        .attr("Context", "nci:ExtCodeID")
                        .attr("Name", &alias.text),
                );
            }
            code.push(node);
        }
        if let Some(external) = &c.external {
            code.push(
                Element::new("ExternalCodeList", &code.path)
                    .attr("Dictionary", &external.dictionary.text)
                    .attr("Version", &external.version.text)
                    .optional("href", external.href.as_ref().map(|t| &t.text)),
            );
        }
        if let Some(alias) = &c.alias {
            code.push(
                Element::new("Alias", &code.path)
                    .attr("Context", "nci:ExtCodeID")
                    .attr("Name", &alias.text),
            );
        }
        metadata.push(code);
    }
}

/// The writer's closed element construction fixes official schema order and
/// references. Composition admits required attributes, numeric ranges, types,
/// keys and cross-references; character admission happens before bytes escape.
pub fn render(study: &Study) -> Result<Vec<u8>, Finding> {
    validate_profile(study)?;
    let section = study.section();
    let modern = matches!(section.define_version, ast::project::DefineVersion::V2_1(_));
    let language = &section.xml_lang.text;
    let mut odm = Element::new("ODM", "study")
        .attr("xmlns", "http://www.cdisc.org/ns/odm/v1.3")
        .attr("xmlns:def", section.define_version.namespace())
        .attr("xmlns:xlink", "http://www.w3.org/1999/xlink")
        .attr("ODMVersion", "1.3.2")
        .attr("FileType", "Snapshot")
        .attr("FileOID", study.file_oid())
        .attr("CreationDateTime", study.created())
        .optional("Originator", section.originator.as_ref().map(|v| &v.text))
        .attr("SourceSystem", "yamaa")
        .attr("SourceSystemVersion", study.source_version())
        .optional("def:Context", modern.then_some(section.context.name()));
    let declared = study.study();
    let mut root = Element::new("Study", "study").attr("OID", format!("STDY.{}", declared.id.name));
    let mut globals = Element::new("GlobalVariables", "study");
    for (name, value) in [
        ("StudyName", &declared.name.text),
        ("StudyDescription", &declared.description.text),
        ("ProtocolName", &declared.protocol_name.text),
    ] {
        let mut node = Element::new(name, "study");
        node.text = Some(value.clone());
        globals.push(node);
    }
    root.push(globals);
    let mut metadata = Element::new("MetaDataVersion", "metadata_version")
        .attr("OID", format!("MDV.{}", study.metadata_id()))
        .attr("Name", study.metadata_name())
        .optional("Description", study.metadata_description())
        .attr("def:DefineVersion", section.define_version.name());
    if !modern {
        let guide = &section.standards[0];
        let name = match guide.name.text.as_str() {
            "ADaMIG" => "ADaM-IG",
            "SDTMIG" => "SDTM-IG",
            "SENDIG" => "SEND-IG",
            name => name,
        };
        metadata = metadata
            .attr("def:StandardName", name)
            .attr("def:StandardVersion", &guide.version.text);
    }
    let mut standards = Element::new("def:Standards", "standards");
    for standard in study.standards() {
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
    if modern {
        metadata.push(standards);
    }
    for (kind, name) in [
        (ast::DocumentKind::AnnotatedCrf, "def:AnnotatedCRF"),
        (ast::DocumentKind::Supplemental, "def:SupplementalDoc"),
    ] {
        let mut parent = Element::new(name, "documents");
        for document in &section.documents {
            if document.kind == kind {
                parent.push(
                    Element::new("def:DocumentRef", "documents")
                        .attr("leafID", format!("LF.{}", document.id.name)),
                );
            }
        }
        if !parent.children.is_empty() {
            metadata.push(parent);
        }
    }
    push_value_lists(&mut metadata, study);
    push_where_clauses(&mut metadata, study);
    push_item_groups(&mut metadata, study, language);
    for dataset in study.datasets() {
        for (column, identity) in variants(dataset) {
            let mut item = item_definition(
                column,
                &identity,
                &format!("datasets.{identity}.submission"),
                &format!(
                    "datasets.{}.columns.{}.label",
                    dataset.id(),
                    column.declaration().name.value
                ),
                language,
                section.define_version,
                section.family,
            );
            if !column.values().is_empty() {
                item.push(
                    Element::new("def:ValueListRef", &item.path)
                        .attr("ValueListOID", format!("VL.{identity}")),
                );
            }
            metadata.push(item);
        }
    }
    push_codelists(&mut metadata, study, language);
    for dataset in study.datasets() {
        for (column, name) in variants(dataset) {
            let meta = column
                .declaration()
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
    for dataset in study.datasets() {
        let mut comments: Vec<(String, &s::Comment)> = Vec::new();
        if let Some(c) = &dataset.metadata().comment {
            comments.push((dataset.id().to_owned(), c));
        }
        for (column, name) in variants(dataset) {
            if let Some(c) = &column
                .declaration()
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
    for (index, document) in section.documents.iter().enumerate() {
        metadata.leaf(
            &document.id.name,
            &document.link.text,
            &document.title.text,
            Some(&format!(
                "{}.documents[{index}].title",
                model::family_name(section.family)
            )),
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
    if let Some(reference) = oid_references
        .iter()
        .find(|reference| !identities.contains(*reference))
    {
        return Err(xml_invalid(
            "metadata_version",
            "unresolved generated metadata reference",
            text(reference),
        ));
    }
    if let Some(reference) = references
        .iter()
        .find(|reference| !leaves.contains(*reference))
    {
        return Err(Finding::cause(
            ConditionCode::SubmissionUnknownDocument,
            "documents",
            "unresolved generated leaf reference",
        )
        .with("document", reference.as_str()));
    }
    let mut out = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n".to_string();
    if let Some(stylesheet) = &section.stylesheet {
        out.push_str(&format!(
            "<?xml-stylesheet type=\"text/xsl\" href=\"{}\"?>\n",
            escape(&stylesheet.text, "stylesheet", true)?
        ));
    }
    odm.write(0, &mut out)?;
    Ok(out.into_bytes())
}
fn validate_profile(study: &Study) -> Result<(), Finding> {
    if study.section().define_version == ast::project::DefineVersion::V2_0 {
        validate_legacy_profile(study)?;
    }
    let created = study.created();
    if created.len() != 19 || created.parse::<yamaa_core::temporal::DateTime>().is_err() {
        return Err(xml_invalid("created", "CreationDateTime", text(created)));
    }
    // Complete scalar facets for this closed generated element profile. The
    // immutable writer fixes element order/cardinality and generated references;
    // independent installed tests validate bytes with the unchanged official XSD.
    for standard in study.standards() {
        if StandardName::from_name(&standard.name).is_none() {
            return Err(xml_invalid(
                "standards",
                "StandardName enumeration",
                text(&standard.name),
            ));
        }
        if let Some(set) = standard
            .publishing_set
            .as_ref()
            .filter(|set| PublishingSet::from_name(set).is_none())
        {
            return Err(xml_invalid(
                "standards",
                "StandardPublishingSet enumeration",
                text(set),
            ));
        }
    }
    for list in study.codelists() {
        let declaration = &list.declaration;
        // A codelist with an empty item list names that list.
        if declaration.items.as_ref().is_some_and(Vec::is_empty) {
            return Err(xml_invalid(
                &format!("codelists.{}", declaration.id.name),
                "CodeList requires an item or dictionary",
                yamaa_core::json::Value::Array(Vec::new()),
            ));
        }
        if let Some(format) = declaration.format_name.as_ref().map(|t| &t.text) {
            let mut bytes = format.bytes();
            if format.len() > 8
                || !bytes
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || b"_$".contains(&c))
                || !bytes.all(|c| c.is_ascii_alphanumeric() || b"_.".contains(&c))
            {
                return Err(xml_invalid(
                    &format!("codelists.{}.format_name", declaration.id.name),
                    "SASFormatName facet",
                    text(format),
                ));
            }
        }
    }
    Ok(())
}

/// Refuse facts the older document cannot carry instead of silently
/// discarding them while changing a namespace (DEFINE-29).
fn validate_legacy_profile(study: &Study) -> Result<(), Finding> {
    let invalid =
        |path: &str, detail: &str| xml_invalid(path, "Define-XML 2.0 representation", text(detail));
    if study.section().standards.len() != 1 || study.standards().len() != 1 {
        return Err(invalid(
            "standards",
            "2.0 requires one implementation guide and cannot record terminology releases",
        ));
    }
    if study.section().standards[0].status.name() != "Final" {
        return Err(invalid(
            "standards[0].status",
            "2.0 cannot record a draft or provisional guide status",
        ));
    }
    for dataset in study.datasets() {
        let sub = dataset.metadata();
        let at = format!("datasets.{}.submission", dataset.id());
        if sub.subclass.is_some() || sub.has_no_data.value {
            return Err(invalid(&at, "2.0 has no SubClass or HasNoData declaration"));
        }
        for (column, identity) in variants(dataset) {
            for origin in column.origins() {
                let declared = origin.declaration();
                let implicit_source = match declared.kind.value.name() {
                    "Collected" => Some("Investigator"),
                    "Derived" | "Assigned" | "Other" | "Protocol" => Some("Sponsor"),
                    _ => None,
                };
                if legacy_origin(declared.kind.value, study.section().family).is_none()
                    || origin
                        .source()
                        .is_some_and(|source| Some(source.name()) != implicit_source)
                {
                    return Err(invalid(
                        &format!("datasets.{identity}.submission.origins"),
                        "2.0 cannot represent this origin type or source",
                    ));
                }
            }
        }
    }
    Ok(())
}

/// The 2.0 clinical vocabulary and the separate SEND vocabulary. The XSD
/// leaves Type as text, so validation and rendering share this mapping.
fn legacy_origin(kind: OriginType, family: Submission) -> Option<&'static str> {
    if family == Submission::Send {
        match kind {
            OriginType::Collected => Some("COLLECTED"),
            OriginType::Derived => Some("DERIVED"),
            OriginType::Other => Some("OTHER"),
            OriginType::NotAvailable => Some("NOT AVAILABLE"),
            OriginType::Assigned | OriginType::Predecessor | OriginType::Protocol => None,
        }
    } else {
        match kind {
            OriginType::Collected => Some("CRF"),
            OriginType::Derived => Some("Derived"),
            OriginType::Assigned => Some("Assigned"),
            OriginType::Predecessor => Some("Predecessor"),
            OriginType::Protocol => Some("Protocol"),
            OriginType::Other | OriginType::NotAvailable => None,
        }
    }
}
