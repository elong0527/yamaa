//! Deterministic UTF-8 Define-XML for the shared admitted column metadata profile.
//!
//! yamaa-core owns the composed metadata model (`submission_composition`) and
//! the findings this writer raises (`define_diagnostics`); this writer owns the
//! element tree, its schema checks, escaping and the bytes.
use std::collections::{BTreeMap, BTreeSet};
use yamaa_core::{
    application_issue::{Fault, Step},
    ast,
    cdisc_vocabulary::{DataType, PublishingSet, StandardName},
    define_diagnostics::{xml_invalid, xml_whitespace},
    define_legacy,
    diagnostic::ConditionCode,
    limits::Limits,
    project_environment::Submission,
    specification_submission_document as s,
    submission_composition::{self as model, Finding, Study},
    value::Value,
};
mod analysis;

/// One document reference as `def:DocumentRef` writes it (DEFINE-19): the
/// document's identifier and, when it says where it points, the page
/// reference's `PageRefs`, `Type` and `Title`. A specification's metadata and
/// an analysis results source declare one form, which one writer reads.
struct Reference<'a> {
    document: &'a str,
    pages: Option<(&'a str, &'a str, Option<&'a str>)>,
}
impl<'a> From<&'a s::DocumentReference> for Reference<'a> {
    fn from(reference: &'a s::DocumentReference) -> Self {
        match reference {
            s::DocumentReference::Plain(field) => Self {
                document: &field.value,
                pages: None,
            },
            s::DocumentReference::Class(class) => Self {
                document: &class.document.value,
                pages: class.pages.as_ref().map(|pages| {
                    (
                        pages.refs.value.as_str(),
                        pages.kind.value.name(),
                        pages.title.as_ref().map(|title| title.value.as_str()),
                    )
                }),
            },
        }
    }
}
impl<'a> From<&'a ast::DocumentRef> for Reference<'a> {
    fn from(reference: &'a ast::DocumentRef) -> Self {
        Self {
            document: &reference.document.name,
            pages: reference.pages.as_ref().map(|pages| {
                (
                    pages.refs.text.as_str(),
                    pages.kind.text.as_str(),
                    pages.title.as_ref().map(|title| title.text.as_str()),
                )
            }),
        }
    }
}

struct Element {
    name: &'static str,
    attributes: Vec<(&'static str, String)>,
    children: Vec<Element>,
    text: Option<String>,
    text_path: Option<String>,
    path: String,
    /// The spec path of the declaration an attribute writes as declared,
    /// where its untransportable character is reported; an attribute
    /// without one reports at the element's path.
    attribute_paths: Vec<(&'static str, String)>,
    /// The spec path of the declaration that names the element's `OID`,
    /// where a repeated identifier is reported (DEFINE-10); none for a
    /// generated `OID`.
    declared: Option<String>,
}
impl Element {
    /// The element's own `OID` attribute.
    fn oid(&self) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(name, _)| *name == "OID")
            .map(|(_, value)| value.as_str())
    }
    /// Whether two elements write the same bytes: their names, attributes,
    /// text and children, never the declaration paths they report at.
    fn same_content(&self, other: &Self) -> bool {
        self.name == other.name
            && self.attributes == other.attributes
            && self.text == other.text
            && self.children.len() == other.children.len()
            && self
                .children
                .iter()
                .zip(&other.children)
                .all(|(a, b)| a.same_content(b))
    }
    fn new(name: &'static str, path: &str) -> Self {
        Self {
            name,
            attributes: Vec::new(),
            children: Vec::new(),
            text: None,
            text_path: None,
            path: path.into(),
            attribute_paths: Vec::new(),
            declared: None,
        }
    }
    /// Report a repeated `OID` at the declaration that names it, when one
    /// does (DEFINE-10).
    fn declared_at(mut self, declared: Option<String>) -> Self {
        self.declared = declared;
        self
    }
    fn attr(mut self, name: &'static str, value: impl ToString) -> Self {
        self.attributes.push((name, value.to_string()));
        self
    }
    /// The attribute `name`, reporting an untransportable character at the
    /// field `path` that declares its value, if one does.
    fn attr_at(mut self, name: &'static str, value: impl ToString, path: Option<String>) -> Self {
        if let Some(path) = path {
            self.attribute_paths.push((name, path));
        }
        self.attr(name, value)
    }
    /// The spec path an attribute's untransportable character is reported at.
    fn attribute_path(&self, name: &str) -> &str {
        self.attribute_paths
            .iter()
            .find(|(attribute, _)| *attribute == name)
            .map_or(&self.path, |(_, path)| path)
    }
    fn optional(mut self, name: &'static str, value: Option<impl ToString>) -> Self {
        if let Some(value) = value {
            self.attributes.push((name, value.to_string()));
        }
        self
    }
    /// The optional attribute `name`, reporting an untransportable character
    /// at the field `path` that declares its value.
    fn optional_at(self, name: &'static str, value: Option<impl ToString>, path: String) -> Self {
        match value {
            Some(value) => self.attr_at(name, value, Some(path)),
            None => self,
        }
    }
    fn push(&mut self, child: Self) {
        self.children.push(child);
    }
    fn translated(&mut self, name: &'static str, text: &str, language: Option<&str>) {
        self.translated_at(name, text, language, None);
    }
    fn translated_at(
        &mut self,
        name: &'static str,
        text: &str,
        language: Option<&str>,
        text_path: Option<&str>,
    ) {
        let mut wrapper = Self::new(name, &self.path);
        // `xml:lang` is optional in ODM 1.3.2, and the section's scope may
        // leave it out (DEFINE-21).
        let mut translated = Self::new("TranslatedText", &self.path).optional("xml:lang", language);
        translated.text = Some(text.into());
        translated.text_path = text_path.map(String::from);
        wrapper.push(translated);
        self.push(wrapper);
    }
    /// One `def:DocumentRef` per reference of the declaration's list at
    /// `at`, each reported at its own position in that list.
    fn refs<'a, R: Into<Reference<'a>>>(&mut self, refs: impl IntoIterator<Item = R>, at: &str) {
        for (index, reference) in refs.into_iter().enumerate() {
            let at = format!("{at}[{index}]");
            let Reference { document, pages } = reference.into();
            let mut node =
                Self::new("def:DocumentRef", &at).attr("leafID", format!("LF.{document}"));
            if let Some((refs, kind, title)) = pages {
                let pages = format!("{at}.pages");
                node.push(
                    Self::new("def:PDFPageRef", &pages)
                        .attr_at("PageRefs", refs, Some(format!("{pages}.refs")))
                        .attr("Type", kind)
                        .optional_at("Title", title, format!("{pages}.title")),
                );
            }
            self.push(node);
        }
    }
    /// A `def:leaf` declared at `at`: its `xlink:href`, reported at the
    /// field that writes it, and its title.
    fn leaf(
        &mut self,
        id: &str,
        (href, href_path): (&str, &str),
        title: &str,
        at: &str,
        title_path: Option<&str>,
    ) {
        let mut node = Self::new("def:leaf", at)
            .attr("ID", format!("LF.{id}"))
            .attr_at("xlink:href", href, Some(href_path.to_owned()));
        let mut child = Self::new("def:title", at);
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
                &format!(
                    " {name}=\"{}\"",
                    escape(value, self.attribute_path(name), true)?
                ),
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
    /// Each `OID` or `ID` maps to the declaration that names it, if any: a
    /// repeated identifier is reported at the later element's declaration,
    /// else at the earlier one's, else at the later element (DEFINE-10).
    fn validate(
        &self,
        identities: &mut BTreeMap<String, Option<String>>,
        leaves: &mut BTreeSet<String>,
        references: &mut Vec<(String, String)>,
        oid_references: &mut Vec<(String, String)>,
    ) -> Result<(), Finding> {
        // Each refusal names the attribute value that breaks the schema.
        let invalid = |facet, value: &str| xml_invalid(&self.path, facet, text(value));
        for (name, value) in &self.attributes {
            match *name {
                "Name" | "Role" if value.is_empty() => return Err(invalid("nonempty name", value)),
                "OID" | "ID" => {
                    if let Some(earlier) = identities.get(value) {
                        let at = self.declared.as_ref().or(earlier.as_ref());
                        return Err(Finding::cause(
                            ConditionCode::SubmissionDuplicateIdentifier,
                            at.unwrap_or(&self.path),
                            value,
                        )
                        .with("identifier", value.as_str()));
                    }
                    identities.insert(value.clone(), self.declared.clone());
                    if *name == "ID" {
                        if !ncname(value) {
                            return Err(invalid("leaf ID NCName", value));
                        }
                        leaves.insert(value.clone());
                    }
                }
                "leafID" | "def:ArchiveLocationID" => {
                    references.push((value.clone(), self.path.clone()));
                }
                "ItemOID" | "def:ItemOID" | "CodeListOID" | "MethodOID" | "def:StandardOID"
                | "def:CommentOID" | "ValueListOID" | "WhereClauseOID" | "ItemGroupOID"
                | "ParameterOID" => oid_references.push((value.clone(), self.path.clone())),
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
impl Element {
    /// A page reference's `Title` is a Define-XML 2.1 attribute: each one the
    /// tree writes is refused at the `pages.title` that declares it, with the
    /// title as `value`, once however many definitions write it (DEFINE-29).
    fn legacy_titles(&self, refusals: &mut Vec<Finding>) {
        if self.name == "def:PDFPageRef" {
            if let Some((_, title)) = self.attributes.iter().find(|(name, _)| *name == "Title") {
                let at = self.attribute_path("Title");
                if !refusals.iter().any(|refused| refused.path == at) {
                    refusals.push(xml_invalid(at, define_legacy::FACET, text(title)));
                }
            }
        }
        for child in &self.children {
            child.legacy_titles(refusals);
        }
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
/// Write the comment declared at `node`'s path into `node`.
fn comment(node: &mut Element, comment: &s::Comment, language: Option<&str>) {
    let at = node.path.clone();
    match comment {
        s::Comment::Plain(text) => {
            described(node, &text.value, None::<&s::DocumentReference>, language);
        }
        s::Comment::Class(class) => {
            node.translated_at(
                "Description",
                &class.text.value,
                language,
                Some(&format!("{at}.text")),
            );
            node.refs(&class.documents.value, &format!("{at}.documents"));
        }
    }
}
/// Write a comment's content into `node` (DEFINE-21): its text as the one
/// `Description`, then one `def:DocumentRef` per document it refers to, each
/// reported in the `documents` of the declaration at `node`'s path. A
/// `def:CommentDef` of a specification, an analysis join comment and an
/// `arm:Documentation` hold the one content.
fn described<'a, R: Into<Reference<'a>>>(
    node: &mut Element,
    text: &str,
    documents: impl IntoIterator<Item = R>,
    language: Option<&str>,
) {
    node.translated("Description", text, language);
    let at = format!("{}.documents", node.path);
    node.refs(documents, &at);
}
/// A method's OID: the one a reproduced declaration names, otherwise the
/// generated `MT.<identity>` (DEFINE-9).
fn method_oid(method: &s::Method, identity: &str) -> String {
    method
        .oid()
        .map_or_else(|| format!("MT.{identity}"), str::to_owned)
}
/// A comment's OID: the one a reproduced declaration names, otherwise the
/// generated `COM.<identity>` (DEFINE-9).
fn comment_oid(comment: &s::Comment, identity: &str) -> String {
    comment
        .oid()
        .map_or_else(|| format!("COM.{identity}"), str::to_owned)
}
/// Append a method, comment or where-clause definition unless an earlier one
/// declared the same OID and writes the same element, which it then shares
/// (DEFINE-30).
/// Any other repeated OID stays, and the document's identifier check refuses
/// it as `duplicate_define_identifier` (DEFINE-10).
fn push_shared(
    metadata: &mut Element,
    written: &mut Vec<(String, bool)>,
    node: Element,
    declared: bool,
) {
    let oid = node
        .attributes
        .iter()
        .find(|(name, _)| *name == "OID")
        .map(|(_, value)| value.clone())
        .expect("definition OID");
    let shared = declared
        && written.iter().any(|(earlier, was_declared)| {
            *was_declared
                && *earlier == oid
                && metadata
                    .children
                    .iter()
                    .any(|child| child.oid() == Some(oid.as_str()) && child.same_content(&node))
        });
    if !shared {
        written.push((oid, declared));
        metadata.push(node);
    }
}
/// The submission metadata of a composed column or value-level entry.
fn submission(declaration: &s::Column) -> &s::ColumnSubmission {
    declaration.submission.as_ref().expect("composed column")
}
/// One base or per-value definition of a dataset, in `ItemDef` order: its
/// column metadata, the column a value-level entry belongs to, the identity
/// its generated identifiers close with (DEFINE-9) and what a reproduced
/// entry keeps.
struct Variant<'a> {
    column: &'a model::Column,
    /// The column whose entry this is; none for the column itself.
    base: Option<&'a model::Column>,
    identity: String,
    received: Option<&'a model::Received>,
}
impl Variant<'_> {
    /// The definition whose declaration writes what `field` reads: the
    /// column, for a field a value-level entry takes from it (DEFINE-27),
    /// otherwise the definition itself. A field the entry writes itself has
    /// its own position, so it never equals the column's.
    fn declaring<T: PartialEq + ?Sized>(&self, field: fn(&s::Column) -> &T) -> &model::Column {
        match self.base {
            Some(base) if field(base.declaration()) == field(self.column.declaration()) => base,
            _ => self.column,
        }
    }
    /// Whether the definition writes what `field` reads itself.
    fn writes<T: PartialEq + ?Sized>(&self, field: fn(&s::Column) -> &T) -> bool {
        std::ptr::eq(self.declaring(field), self.column)
    }
}
/// One `ItemDef`, base or per value (DEFINE-19), reported at its declaration;
/// its description at the `label` that declares it, the entry's own or its
/// column's.
fn item_definition(
    dataset: &model::Dataset,
    variant: &Variant<'_>,
    language: Option<&str>,
    section: &ast::Section,
) -> Element {
    let Variant {
        column,
        ref identity,
        received,
        ..
    } = *variant;
    let (version, family) = (section.define_version, section.family);
    let modern = matches!(version, ast::project::DefineVersion::V2_1(_));
    let name = &column.declaration().name.value;
    let meta = submission(column.declaration());
    // A reproduced entry may keep its received OID and Name; the transport
    // name stays the column's (DEFINE-30).
    let declared = received
        .and_then(|received| received.oid.as_ref())
        .map(|_| column.path().to_owned());
    let mut item = Element::new("ItemDef", column.path())
        .declared_at(declared)
        .attr("OID", item_oid(identity, received))
        .attr(
            "Name",
            received
                .and_then(|received| received.name.as_deref())
                .unwrap_or(name),
        )
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
            meta.comment.as_ref().map(|c| comment_oid(c, identity)),
        );
    // Every validated definition has a description; a reproduced per-value
    // definition has one only when its entry declares a `label` (DEFINE-30).
    if let Some(label) = &column.declaration().label {
        let at = if variant.base.is_some() && variant.writes(|c| &c.label) {
            format!("{}.label", column.path())
        } else {
            format!("{}.columns.{name}.label", dataset.path())
        };
        item.translated_at("Description", &label.value, language, Some(&at));
    }
    if let Some(list) = &meta.codelist {
        item.push(
            Element::new("CodeListRef", &item.path)
                .attr("CodeListOID", format!("CL.{}", list.value)),
        );
    }
    // One `def:Origin` per declared origin, in declaration order (DEFINE-19).
    let declaring = variant.declaring(|c| &submission(c).origins);
    for (index, origin) in column.origins().iter().enumerate() {
        let declared = origin.declaration();
        let at = format!("{}.origins[{index}]", declaring.path());
        let mut node = Element::new("def:Origin", &at)
            .attr(
                "Type",
                if modern {
                    declared.kind.value.name()
                } else {
                    // An origin 2.0 cannot record is refused before any
                    // byte is written.
                    define_legacy::origin(declared.kind.value, origin.source(), family)
                        .unwrap_or_else(|_| declared.kind.value.name())
                },
            )
            .optional(
                "Source",
                origin.source().filter(|_| modern).map(|v| v.name()),
            );
        if let Some(description) = &declared.description {
            node.translated_at(
                "Description",
                &description.value,
                language,
                Some(&format!("{at}.description")),
            );
        }
        node.refs(origin.documents(), &format!("{at}.documents"));
        item.push(node);
    }
    item
}
/// An `ItemDef`'s OID: the one a reproduced entry declares, otherwise the
/// generated `IT.<identity>` (DEFINE-9).
fn item_oid(identity: &str, received: Option<&model::Received>) -> String {
    received
        .and_then(|received| received.oid.clone())
        .unwrap_or_else(|| format!("IT.{identity}"))
}
/// A where clause's OID: the one a reproduced declaration names, otherwise
/// the generated `<prefix>.<identity>` (DEFINE-9).
fn where_oid(declared: Option<&str>, prefix: &str, identity: &str) -> String {
    declared.map_or_else(|| format!("{prefix}.{identity}"), str::to_owned)
}
/// Every base and per-value definition of a dataset, in `ItemDef` order: each
/// column followed by its value-level entries, with the entry `id` closing
/// the per-value identity (DEFINE-9), and what a reproduced entry keeps.
fn variants(dataset: &model::Dataset) -> impl Iterator<Item = Variant<'_>> {
    dataset.columns().iter().flat_map(move |column| {
        let identity = format!("{}.{}", dataset.id(), column.declaration().name.value);
        let base = Variant {
            column,
            base: None,
            identity: identity.clone(),
            received: None,
        };
        std::iter::once(base).chain(column.values().iter().map(move |value| Variant {
            column: value.column(),
            base: Some(column),
            identity: format!("{identity}.{}", value.id()),
            received: Some(value.received()),
        }))
    })
}

/// Append one value-list definition per column that declares values, in
/// dataset and column order.
fn push_value_lists(metadata: &mut Element, study: &Study) {
    let modern = matches!(
        study.section().define_version,
        ast::project::DefineVersion::V2_1(_)
    );
    for dataset in study.datasets() {
        for column in dataset.columns() {
            if column.values().is_empty() {
                continue;
            }
            let identity = format!("{}.{}", dataset.id(), column.declaration().name.value);
            let at = format!("{}.value_level", column.path());
            let mut list =
                Element::new("def:ValueListDef", &at).attr("OID", format!("VL.{identity}"));
            for (index, value) in column.values().iter().enumerate() {
                let identity = format!("{identity}.{}", value.id());
                let meta = value
                    .column()
                    .declaration()
                    .submission
                    .as_ref()
                    .expect("composed value");
                let received = value.received();
                let mut reference = Element::new("ItemRef", value.path())
                    .attr("ItemOID", item_oid(&identity, Some(received)))
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
                        meta.method.as_ref().map(|m| method_oid(m, &identity)),
                    )
                    // No row the entry selects holds a value (SUB-28).
                    .optional(
                        "def:HasNoData",
                        (modern && value.column().has_no_data()).then_some("Yes"),
                    );
                reference.push(Element::new("def:WhereClauseRef", value.path()).attr(
                    "WhereClauseOID",
                    where_oid(received.where_oid.as_deref(), "WC", &identity),
                ));
                list.push(reference);
            }
            metadata.push(list);
        }
    }
}

/// Append one where-clause definition per declared value, in dataset,
/// column and value order, then the analysis datasets' (DEFINE-11). Where
/// clauses that declare one received OID with the same range checks share
/// one definition (DEFINE-10).
fn push_where_clauses(metadata: &mut Element, study: &Study) {
    let mut written = Vec::new();
    for dataset in study.datasets() {
        for column in dataset.columns() {
            for value in column.values() {
                let identity = format!(
                    "{}.{}.{}",
                    dataset.id(),
                    column.declaration().name.value,
                    value.id()
                );
                // A received OID is reported at the `where_oid` that
                // declares it (DEFINE-10).
                let declared = value.received().where_oid.as_deref();
                let at = format!("{}.where", value.path());
                let mut clause = Element::new("def:WhereClauseDef", &at)
                    .declared_at(declared.map(|_| format!("{}.where_oid", value.path())))
                    .attr("OID", where_oid(declared, "WC", &identity));
                for term in value.clause() {
                    // An ordered term's one value is the value composition
                    // converted.
                    let typed = term.bound.as_ref();
                    clause.push(range_check(&at, dataset, term, typed));
                }
                push_shared(metadata, &mut written, clause, declared.is_some());
            }
        }
    }
    analysis::push_where_clauses(metadata, study, &mut written);
}

/// One `RangeCheck` of a where clause, a value-level entry's or an analysis
/// dataset's (DEFINE-27): one value is an equality and several a membership,
/// an exclusion is NE for one value and NOTIN for several, and an ordered
/// term writes its own comparator. `typed` gives the term's values as the
/// where column's type holds them, where composition converted them. `at` is
/// the clause's `where`, which names the where column.
fn range_check<'v>(
    at: &str,
    dataset: &model::Dataset,
    term: &yamaa_core::dataset::WhereTerm,
    typed: impl IntoIterator<Item = &'v Value>,
) -> Element {
    let where_column = &dataset.columns()[term.column];
    let target = &where_column.declaration().name.value;
    let path = &format!("{at}.{target}");
    let mut check = Element::new("RangeCheck", path)
        .attr("Comparator", term.comparator())
        .attr("SoftHard", "Soft")
        .attr("def:ItemOID", format!("IT.{}.{target}", dataset.id()));
    // A converted value is written as the where column's ItemDef types it:
    // an ADaM date, datetime or time as its SAS number (SUB-25), any other
    // as written.
    let mut typed = typed.into_iter();
    for written in &term.values {
        let submitted = typed
            .next()
            .filter(|_| where_column.data_type() == DataType::Integer)
            .and_then(|value| crate::epoch::sas_number(value.into()));
        let mut code = Element::new("CheckValue", path);
        code.text = Some(submitted.map_or_else(|| written.clone(), |number| number.to_string()));
        check.push(code);
    }
    check
}

/// Append one item-group definition per dataset, in dataset order.
fn push_item_groups(metadata: &mut Element, study: &Study, language: Option<&str>) {
    let section = study.section();
    let modern = matches!(section.define_version, ast::project::DefineVersion::V2_1(_));
    for dataset in study.datasets() {
        let id = dataset.id();
        let sub = dataset.metadata();
        let at = format!("{}.submission", dataset.path());
        let mut group = Element::new("ItemGroupDef", &at)
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
            .attr_at(
                "def:Structure",
                &sub.structure.value,
                Some(format!("{at}.structure")),
            )
            .optional("def:Class", (!modern).then_some(sub.class.value.name()))
            // A dataset with no data and no transport file has no leaf.
            .optional(
                "def:ArchiveLocationID",
                dataset.href().map(|_| format!("LF.{id}")),
            )
            // The guide the dataset selects, or the section's first; a
            // non-standard dataset binds none and says so (DEFINE-6).
            .optional(
                "def:StandardOID",
                dataset
                    .standard()
                    .filter(|_| modern)
                    .map(|standard| format!("STD.{standard}")),
            )
            .optional(
                "def:IsNonStandard",
                (modern && sub.non_standard.value).then_some("Yes"),
            )
            .optional(
                "def:CommentOID",
                sub.comment.as_ref().map(|c| comment_oid(c, id)),
            )
            .optional("def:HasNoData", sub.has_no_data.value.then_some("Yes"));
        group.translated_at(
            "Description",
            &sub.label.value,
            language,
            Some(&format!("{at}.label")),
        );
        // Each `ItemRef` takes the `OrderNumber` reproduced metadata declares,
        // which composition requires of every column or none, or else its
        // position; the references are written in that order (DEFINE-18).
        let mut columns: Vec<_> = dataset
            .columns()
            .iter()
            .enumerate()
            .map(|(index, column)| (column.order().unwrap_or(index + 1), column))
            .collect();
        columns.sort_by_key(|(order, _)| *order);
        for (order, column) in columns {
            let name = &column.declaration().name.value;
            let meta = column
                .declaration()
                .submission
                .as_ref()
                .expect("composed column");
            group.push(
                Element::new("ItemRef", column.path())
                    .attr("ItemOID", format!("IT.{id}.{name}"))
                    .attr("OrderNumber", order)
                    .attr("Mandatory", yes(column.mandatory()))
                    .optional("KeySequence", column.key_sequence())
                    .optional("Role", meta.role.as_ref().map(|v| &v.value))
                    .optional(
                        "MethodOID",
                        meta.method
                            .as_ref()
                            .map(|m| method_oid(m, &format!("{id}.{name}"))),
                    )
                    // No published row holds a value in the column (SUB-28).
                    .optional(
                        "def:HasNoData",
                        (modern && column.has_no_data()).then_some("Yes"),
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
        // The leaf of the dataset's transport file, named for its listing,
        // or of the literal artifact a reproduced dataset declares, which
        // writes both its link and its title (DEFINE-16, DEFINE-30).
        if let Some(href) = dataset.href() {
            let title = href.rsplit('/').next().unwrap_or(href);
            let written = sub
                .artifact
                .as_ref()
                .map_or_else(|| dataset.path().to_owned(), |_| format!("{at}.artifact"));
            group.leaf(id, (href, &written), title, dataset.path(), Some(&written));
        }
        metadata.push(group);
    }
}

/// Append one code-list definition per bound codelist, in catalogue order.
fn push_codelists(metadata: &mut Element, study: &Study, language: Option<&str>) {
    let modern = matches!(
        study.section().define_version,
        ast::project::DefineVersion::V2_1(_)
    );
    for list in study.codelists() {
        let c = &list.declaration;
        let at = &list.path;
        let mut code = Element::new("CodeList", at)
            .attr("OID", format!("CL.{}", c.id.name))
            .attr_at("Name", &c.name.text, Some(format!("{at}.name")))
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
        for (index, item) in c.items.as_deref().unwrap_or(&[]).iter().enumerate() {
            let item_at = format!("{at}.items[{index}]");
            let mut node = Element::new(
                if item.decode.is_some() {
                    "CodeListItem"
                } else {
                    "EnumeratedItem"
                },
                &item_at,
            )
            .attr_at(
                "CodedValue",
                scalar(&item.value),
                Some(format!("{item_at}.value")),
            )
            .optional("Rank", item.rank)
            .optional("OrderNumber", item.order)
            .optional("def:ExtendedValue", item.extended.then_some("Yes"));
            if let Some(decode) = &item.decode {
                let decode_at = format!("{item_at}.decode");
                node.translated_at("Decode", &decode.text, language, Some(&decode_at));
            }
            if let Some(alias) = &item.alias {
                node.push(
                    Element::new("Alias", &format!("{item_at}.alias"))
                        .attr("Context", "nci:ExtCodeID")
                        .attr("Name", &alias.text),
                );
            }
            code.push(node);
        }
        if let Some(external) = &c.external {
            code.push(
                Element::new("ExternalCodeList", &format!("{at}.external"))
                    .attr("Dictionary", &external.dictionary.text)
                    .attr("Version", &external.version.text)
                    .optional("href", external.href.as_ref().map(|t| &t.text)),
            );
        }
        if let Some(alias) = &c.alias {
            code.push(
                Element::new("Alias", &format!("{at}.alias"))
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
/// Every fact the selected release cannot represent is refused, each
/// reported (DEFINE-29): the declarations a Define-XML 2.0 document cannot
/// carry, then each 2.1 attribute the element tree writes; only then is the
/// first value outside a schema facet refused.
pub fn render(study: &Study) -> Result<Vec<u8>, Vec<Finding>> {
    let legacy = study.section().define_version == ast::project::DefineVersion::V2_0;
    let mut refusals = if legacy {
        legacy_refusals(study)
    } else {
        Vec::new()
    };
    let odm = build(study);
    if legacy {
        odm.legacy_titles(&mut refusals);
    }
    if !refusals.is_empty() {
        return Err(refusals);
    }
    validate_facets(study).map_err(|finding| vec![finding])?;
    write(study, &odm).map_err(|finding| vec![finding])
}
/// The element tree of a study's document.
fn build(study: &Study) -> Element {
    let section = study.section();
    let modern = matches!(section.define_version, ast::project::DefineVersion::V2_1(_));
    // The translated text outside and inside the analysis results metadata
    // carries `xml:lang` as the section's scope says (DEFINE-21).
    let scope = section.xml_lang_scope;
    let language = scope.document().then_some(section.xml_lang.text.as_str());
    let results_language = scope
        .analysis_results()
        .then_some(section.xml_lang.text.as_str());
    let key = model::family_name(section.family);
    let mut odm = Element::new("ODM", key)
        .attr("xmlns", "http://www.cdisc.org/ns/odm/v1.3")
        .attr("xmlns:def", section.define_version.namespace())
        .attr("xmlns:xlink", "http://www.w3.org/1999/xlink")
        // The one extension namespace, declared only by a document that
        // carries analysis results metadata (DEFINE-11).
        .optional(
            "xmlns:arm",
            (!study.analysis_displays().is_empty()).then_some(analysis::NAMESPACE),
        )
        .attr("ODMVersion", "1.3.2")
        .attr("FileType", "Snapshot")
        .attr_at(
            "FileOID",
            study.file_oid(),
            declared_field(section, section.file_oid.as_ref(), "file_oid"),
        )
        .attr("CreationDateTime", study.created())
        .optional_at(
            "Originator",
            section.originator.as_ref().map(|v| &v.text),
            format!("{key}.originator"),
        )
        .attr_at(
            "SourceSystem",
            study.source_system(),
            declared_field(section, section.source_system.as_ref(), "source_system"),
        )
        .attr_at(
            "SourceSystemVersion",
            study.source_version(),
            declared_field(
                section,
                section.source_system_version.as_ref(),
                "source_system_version",
            ),
        )
        .optional("def:Context", modern.then_some(section.context.name()));
    let declared = study.study();
    let mut root = Element::new("Study", "study").attr_at(
        "OID",
        study.study_oid(),
        declared_field(section, section.study_oid.as_ref(), "study_oid"),
    );
    let mut globals = Element::new("GlobalVariables", "study");
    for (name, value, field) in [
        ("StudyName", &declared.name.text, "name"),
        (
            "StudyDescription",
            &declared.description.text,
            "description",
        ),
        (
            "ProtocolName",
            &declared.protocol_name.text,
            "protocol_name",
        ),
    ] {
        let mut node = Element::new(name, "study");
        node.text = Some(value.clone());
        node.text_path = Some(format!("study.{field}"));
        globals.push(node);
    }
    root.push(globals);
    let mut metadata = metadata_version(study, modern);
    if modern {
        metadata.push(standards(study));
    }
    push_documents(&mut metadata, section);
    push_value_lists(&mut metadata, study);
    push_where_clauses(&mut metadata, study);
    push_item_groups(&mut metadata, study, language);
    push_items(&mut metadata, study, language);
    push_codelists(&mut metadata, study, language);
    push_methods(&mut metadata, study, language);
    push_comments(&mut metadata, study, language);
    analysis::push_join_comments(&mut metadata, study, language);
    for (index, document) in section.documents.iter().enumerate() {
        let at = format!("{key}.documents[{index}]");
        metadata.leaf(
            &document.id.name,
            (&document.link.text, &format!("{at}.link")),
            &document.title.text,
            &at,
            Some(&format!("{at}.title")),
        );
    }
    analysis::push_displays(&mut metadata, study, results_language);
    root.push(metadata);
    odm.push(root);
    odm
}
/// The bytes of the document `odm` holds, once every identifier and
/// reference is checked and every character admitted.
fn write(study: &Study, odm: &Element) -> Result<Vec<u8>, Finding> {
    let section = study.section();
    let key = model::family_name(section.family);
    resolve(odm)?;
    let mut out = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n".to_string();
    if let Some(stylesheet) = &section.stylesheet {
        out.push_str(&format!(
            "<?xml-stylesheet type=\"text/xsl\" href=\"{}\"?>\n",
            escape(&stylesheet.text, &format!("{key}.stylesheet"), true)?
        ));
    }
    odm.write(0, &mut out)?;
    Ok(out.into_bytes())
}
/// Every identifier is unique and every reference resolves (DEFINE-10): an
/// unresolved reference is reported at the declaration of the element that
/// writes it.
fn resolve(odm: &Element) -> Result<(), Finding> {
    let mut identities = BTreeMap::new();
    let mut leaves = BTreeSet::new();
    let mut references = Vec::new();
    let mut oid_references = Vec::new();
    odm.validate(
        &mut identities,
        &mut leaves,
        &mut references,
        &mut oid_references,
    )?;
    if let Some((reference, at)) = oid_references
        .iter()
        .find(|(reference, _)| !identities.contains_key(reference))
    {
        return Err(xml_invalid(
            at,
            "unresolved generated metadata reference",
            text(reference),
        ));
    }
    if let Some((reference, at)) = references
        .iter()
        .find(|(reference, _)| !leaves.contains(reference))
    {
        return Err(Finding::cause(
            ConditionCode::SubmissionUnknownDocument,
            at,
            "unresolved generated leaf reference",
        )
        .with("document", reference.as_str()));
    }
    Ok(())
}
/// `def:Standards`, written by a Define-XML 2.1 document: each standard at
/// the declaration that names it, a section guide or a codelist source's
/// terminology standard.
fn standards(study: &Study) -> Element {
    let key = model::family_name(study.section().family);
    let mut standards = Element::new("def:Standards", &format!("{key}.standards"));
    for standard in study.standards() {
        let at = &standard.path;
        standards.push(
            Element::new("def:Standard", at)
                .attr("OID", format!("STD.{}", standard.id))
                .attr_at("Name", &standard.name, Some(format!("{at}.name")))
                .attr("Type", standard.kind)
                .optional_at(
                    "PublishingSet",
                    standard.publishing_set.as_ref(),
                    format!("{at}.publishing_set"),
                )
                .attr_at("Version", &standard.version, Some(format!("{at}.version")))
                .attr("Status", standard.status),
        );
    }
    standards
}
/// The annotated case report form and the supplemental documents, each
/// reference at the section's `documents` entry that declares it.
fn push_documents(metadata: &mut Element, section: &ast::Section) {
    let key = model::family_name(section.family);
    for (kind, name) in [
        (ast::DocumentKind::AnnotatedCrf, "def:AnnotatedCRF"),
        (ast::DocumentKind::Supplemental, "def:SupplementalDoc"),
    ] {
        let mut parent = Element::new(name, &format!("{key}.documents"));
        for (index, document) in section.documents.iter().enumerate() {
            if document.kind == kind {
                parent.push(
                    Element::new("def:DocumentRef", &format!("{key}.documents[{index}]"))
                        .attr("leafID", format!("LF.{}", document.id.name)),
                );
            }
        }
        if !parent.children.is_empty() {
            metadata.push(parent);
        }
    }
}
/// Append every base and per-value `ItemDef`, in dataset and column order.
fn push_items(metadata: &mut Element, study: &Study, language: Option<&str>) {
    for dataset in study.datasets() {
        for variant in variants(dataset) {
            let mut item = item_definition(dataset, &variant, language, study.section());
            if !variant.column.values().is_empty() {
                item.push(
                    Element::new("def:ValueListRef", &item.path)
                        .attr("ValueListOID", format!("VL.{}", variant.identity)),
                );
            }
            metadata.push(item);
        }
    }
}
/// Append one `MethodDef` per definition that declares a method, each at
/// the `method` that declares it, a value-level entry's own or its
/// column's.
fn push_methods(metadata: &mut Element, study: &Study, language: Option<&str>) {
    let mut written = Vec::new();
    // A declared OID whose methods declare no `Name` takes the one generated
    // for its first method, so identical declarations share it (DEFINE-30).
    let mut default_names = BTreeMap::new();
    for dataset in study.datasets() {
        for variant in variants(dataset) {
            let name = &variant.identity;
            let Some(method) = &submission(variant.column.declaration()).method else {
                continue;
            };
            let at = format!(
                "{}.method",
                variant.declaring(|c| &submission(c).method).path()
            );
            let mut node = Element::new("MethodDef", &at)
                .declared_at(method.oid().map(|_| at.clone()))
                .attr("OID", method_oid(method, name));
            match method {
                s::Method::Plain(description) => {
                    node = node.attr("Name", name).attr("Type", "Computation");
                    node.translated("Description", &description.value, language);
                }
                s::Method::Class(class) => {
                    let generated = match (&class.oid, &class.name) {
                        (Some(oid), None) => default_names
                            .entry(oid.value.as_str())
                            .or_insert_with(|| name.clone()),
                        _ => name,
                    };
                    node = node
                        .attr_at(
                            "Name",
                            class.name.as_ref().map_or(generated, |v| &v.value),
                            class.name.as_ref().map(|_| format!("{at}.name")),
                        )
                        .attr("Type", class.kind.value.name());
                    node.translated_at(
                        "Description",
                        &class.description.value,
                        language,
                        Some(&format!("{at}.description")),
                    );
                    if let Some(expression) = &class.expression {
                        let code = format!("{at}.expression");
                        let mut formal = Element::new("FormalExpression", &code).attr_at(
                            "Context",
                            &expression.context.value,
                            Some(format!("{code}.context")),
                        );
                        formal.text = Some(expression.code.value.clone());
                        formal.text_path = Some(format!("{code}.code"));
                        node.push(formal);
                    }
                    node.refs(&class.documents.value, &format!("{at}.documents"));
                }
            }
            push_shared(metadata, &mut written, node, method.oid().is_some());
        }
    }
}
/// Append one `def:CommentDef` per dataset and definition that declares a
/// comment, each at the `comment` that declares it.
fn push_comments(metadata: &mut Element, study: &Study, language: Option<&str>) {
    let mut written = Vec::new();
    for dataset in study.datasets() {
        let mut comments: Vec<(String, String, &s::Comment)> = Vec::new();
        if let Some(c) = &dataset.metadata().comment {
            let at = format!("{}.submission.comment", dataset.path());
            comments.push((dataset.id().to_owned(), at, c));
        }
        for variant in variants(dataset) {
            if let Some(c) = &submission(variant.column.declaration()).comment {
                let declaring = variant.declaring(|d| &submission(d).comment);
                let at = format!("{}.comment", declaring.path());
                comments.push((variant.identity.clone(), at, c));
            }
        }
        for (name, at, c) in comments {
            let mut node = Element::new("def:CommentDef", &at)
                .declared_at(c.oid().map(|_| at.clone()))
                .attr("OID", comment_oid(c, &name));
            comment(&mut node, c, language);
            push_shared(metadata, &mut written, node, c.oid().is_some());
        }
    }
}
/// The `MetaDataVersion` element's own attributes: its identity, and the
/// implementation guide a Define-XML 2.0 document names on it.
fn metadata_version(study: &Study, modern: bool) -> Element {
    let section = study.section();
    let key = model::family_name(section.family);
    // The section's `metadata_version` when it declares one, else the
    // section, whose study the generated name and OID describe.
    let declared = section.metadata_version.as_ref();
    let at = declared.map_or_else(|| key.to_owned(), |_| format!("{key}.metadata_version"));
    let metadata = Element::new("MetaDataVersion", &at)
        .attr_at(
            "OID",
            study.metadata_oid(),
            declared_field(
                section,
                section
                    .metadata_version
                    .as_ref()
                    .and_then(|m| m.oid.as_ref()),
                "metadata_version.oid",
            ),
        )
        .attr_at(
            "Name",
            study.metadata_name(),
            declared.map(|_| format!("{at}.name")),
        )
        .optional_at(
            "Description",
            study.metadata_description(),
            format!("{at}.description"),
        )
        .attr("def:DefineVersion", section.define_version.name());
    if !modern {
        let guide = &section.standards[0];
        let name = match guide.name.text.as_str() {
            "ADaMIG" => "ADaM-IG",
            "SDTMIG" => "SDTM-IG",
            "SENDIG" => "SEND-IG",
            name => name,
        };
        return metadata
            .attr("def:StandardName", name)
            .attr("def:StandardVersion", &guide.version.text);
    }
    metadata
}
/// The section's field `name`, when it declares the value an attribute
/// writes: an untransportable character is reported there.
fn declared_field<T>(section: &ast::Section, value: Option<T>, name: &str) -> Option<String> {
    value.map(|_| format!("{}.{name}", model::family_name(section.family)))
}
/// The first generated value outside its Define-XML schema facet.
fn validate_facets(study: &Study) -> Result<(), Finding> {
    let created = study.created();
    if yamaa_core::define_diagnostics::created_text(created).is_err() {
        return Err(xml_invalid("created", "CreationDateTime", text(created)));
    }
    // Complete scalar facets for this closed generated element profile. The
    // immutable writer fixes element order/cardinality and generated references;
    // independent installed tests validate bytes with the unchanged official XSD.
    for standard in study.standards() {
        if StandardName::from_name(&standard.name).is_none() {
            return Err(xml_invalid(
                &format!("{}.name", standard.path),
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
                &format!("{}.publishing_set", standard.path),
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
                &format!("{}.items", list.path),
                "CodeList requires an item or dictionary",
                yamaa_core::json::Value::Array(Vec::new()),
            ));
        }
        // ODM 1.3.2 makes an item's `OrderNumber` unique within its
        // codelist (UC-CL-2, UC-CL-4) in either mode. Validated metadata
        // numbers from 1; a reproduced section writes a received 0 as
        // declared (DEFINE-20, owner decision on #5022).
        let (floor, floor_facet) = if study.reproduced() {
            (0, "non-negative order")
        } else {
            (1, "positive order")
        };
        let mut orders = BTreeSet::new();
        for (index, item) in declaration
            .items
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            let Some(order) = item.order else { continue };
            let facet = if order < floor {
                floor_facet
            } else if !orders.insert(order) {
                "unique order"
            } else {
                continue;
            };
            return Err(xml_invalid(
                &format!("{}.items[{index}].order", list.path),
                facet,
                yamaa_core::json::Value::from(order),
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
                    &format!("{}.format_name", list.path),
                    "SASFormatName facet",
                    text(format),
                ));
            }
        }
    }
    Ok(())
}

/// Refuse facts the older document cannot carry instead of silently
/// discarding them while changing a namespace (DEFINE-29). The one exception
/// is a terminology standard a codelist source names: it resolves that
/// source's codelists (CT-6) and 2.0 omits it, so only the section's
/// implementation guides are judged here, never the study's standards.
/// Each refusal is reported at the declared field, in the project file or
/// the listed specification that writes it, with the declared value 2.0
/// cannot record as its `value` (DEFINE-25), and every one is reported. A
/// value-level entry is judged by what it writes itself: what it takes from
/// its column is refused at the column. A page title is judged where the
/// element tree writes it ([`Element::legacy_titles`]).
fn legacy_refusals(study: &Study) -> Vec<Finding> {
    let mut refusals = Vec::new();
    let mut refuse = |path: String, value: yamaa_core::json::Value| {
        refusals.push(xml_invalid(&path, define_legacy::FACET, value));
    };
    let section = model::family_name(study.section().family);
    let guides = &study.section().standards;
    if guides.len() != 1 {
        let names = guides.iter().map(|guide| text(&guide.name.text)).collect();
        refuse(
            format!("{section}.standards"),
            yamaa_core::json::Value::Array(names),
        );
    }
    if guides.len() == 1 && guides[0].status.name() != "Final" {
        refuse(
            format!("{section}.standards[0].status"),
            text(guides[0].status.name()),
        );
    }
    for dataset in study.datasets() {
        let sub = dataset.metadata();
        let at = format!("{}.submission", dataset.path());
        if let Some(subclass) = &sub.subclass {
            refuse(format!("{at}.subclass"), text(subclass.value.name()));
        }
        if sub.has_no_data.value {
            refuse(
                format!("{at}.has_no_data"),
                yamaa_core::json::Value::Bool(true),
            );
        }
        if sub.non_standard.value {
            refuse(
                format!("{at}.non_standard"),
                yamaa_core::json::Value::Bool(true),
            );
        }
        for variant in variants(dataset) {
            legacy_definition(study, &variant, &mut refuse);
        }
    }
    refusals
}
/// The 2.0 refusals of one base or per-value definition: a declared
/// `has_no_data`, and the type or source of each origin it writes itself.
fn legacy_definition(
    study: &Study,
    variant: &Variant<'_>,
    refuse: &mut impl FnMut(String, yamaa_core::json::Value),
) {
    let column = variant.column;
    let meta = submission(column.declaration());
    // The measured flag is a 2.1 attribute 2.0 omits; a declared one is a
    // fact it cannot carry.
    if meta
        .has_no_data
        .as_ref()
        .is_some_and(|declared| declared.value)
    {
        refuse(
            format!("{}.has_no_data", column.path()),
            yamaa_core::json::Value::Bool(true),
        );
    }
    if variant.writes(|c| &submission(c).origins) {
        // The refusal names the origin's own type or source at its
        // position, so the message states the declared value 2.0 lacks.
        for (index, origin) in column.origins().iter().enumerate() {
            let at = format!("{}.origins[{index}]", column.path());
            let kind = origin.declaration().kind.value;
            let source = origin.source();
            match define_legacy::origin(kind, source, study.section().family) {
                Ok(_) => {}
                Err(define_legacy::Gap::Type) => refuse(format!("{at}.type"), text(kind.name())),
                Err(define_legacy::Gap::Source) => refuse(
                    format!("{at}.source"),
                    text(source.expect("only a declared source misses").name()),
                ),
            }
        }
    }
}
