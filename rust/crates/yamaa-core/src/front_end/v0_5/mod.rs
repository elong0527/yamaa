//! Lower today's admitted specification into the AST.
//!
//! Lowering runs once per spec, after composition and pruning, on the
//! normalized document the specification model admitted. Pruning lowers the
//! composed document before that, only to find what each declaration reads
//! ([`lower_document`]). Each 0.5 form lowers to the node its 1.0 spelling
//! lowers to (#2844, design section 4).
//! Some nodes exist only for this surface and go with it: inputs with
//! [`KeyDecl::Inferred`], lookups with [`JoinKey::Applicable`], a declared
//! `minimum_source_precision`, a written `window.order_by` on
//! `baseline_flag`, and the 0.5 path notation ([`Notation::V0_5`]).
//!
//! The 1.0 front end ([`crate::front_end::v1::spec`]) shares this construct
//! lowering: where the two spellings of one construct differ, the lowering
//! reads the spelling of the surface it was started for ([`Surface`]), so
//! both reach the same node by one code path. P2-13 moves the shared lowering
//! into `v1` when the 0.5 surface goes.
//!
//! Lowering is total over every document the model admits. The model does
//! not check operation payloads, so an operation outside the language
//! lowers to [`ExprKind::Unsupported`], which preparation refuses as before.
//!
//! A project's own files lower into their roots in [`project`], and its
//! study and sections in [`sections`].

mod expressions;
pub mod project;
pub mod sections;
#[cfg(test)]
pub(crate) mod tests;

use crate::ast::{
    Binding, Column, ColumnCheck, ColumnCheckKind, ColumnOrigin, ColumnSubmission, Comment,
    Comparator, Constant, DatasetCheck, DatasetCheckKind, DatasetSubmission, Derivation,
    DocumentRef, EmptyString, Expr, ExprId, ExprKind, FileRef, FormalExpression, Header, Input,
    InputSource, Intermediate, JoinKey, KeyDecl, Literal, Meta, Method, Model, ModelKind, Named,
    Notation, Origin, Output, PageRef, PathBase, Paths, Profile, RowCount, Rows, Segment, Severity,
    Span, Spec, SubmissionMetadata, Surface, Template, Text, UniqueCheck, ValueLevel, WhereClause,
    Written,
};
use crate::schema::{Document, DocumentNode as N, SpecificationDocument};
use crate::value::ColumnType;
use std::collections::BTreeSet;

/// Where each node of the admitted document was written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Positions {
    /// The spec's layer list, which [`Span::file`] indexes.
    pub files: Vec<FileRef>,
    /// The written position of each document node. A node a schema default
    /// materialized has none and takes its nearest written ancestor's.
    pub nodes: Vec<Option<Span>>,
}

/// A lowered spec.
#[derive(Clone, Debug)]
pub struct Lowered {
    spec: Spec,
}

impl Lowered {
    pub fn spec(&self) -> &Spec {
        &self.spec
    }
    pub fn into_spec(self) -> Spec {
        self.spec
    }
}

/// Lower one admitted spec. Without positions every span is
/// [`Span::UNKNOWN`].
pub fn lower(spec: &SpecificationDocument, positions: Option<&Positions>) -> Lowered {
    lower_document(spec.document(), positions)
}

/// Lower a document the model has not admitted yet, such as a composed
/// spec before pruning (LAYER-11 precedes requiredness, LAYER-12). Lowering
/// stays total: a missing required field lowers to an unsupported node, and
/// final validation still reports it.
pub fn lower_document(d: &Document, positions: Option<&Positions>) -> Lowered {
    lower_surface(d, positions, Surface::V0_5)
}

/// Lower a document written in `surface`'s spelling. The 1.0 front end
/// enters here ([`crate::front_end::v1::spec::lower`]).
pub(crate) fn lower_surface(
    d: &Document,
    positions: Option<&Positions>,
    surface: Surface,
) -> Lowered {
    let mut lower = Lower {
        d,
        positions,
        surface,
        paths: Paths::new(),
        exprs: Vec::new(),
    };
    let root = d.root();
    let meta = Meta {
        path: lower.paths.root(),
        span: lower.span(Some(root), Span::UNKNOWN),
    };
    let mut spec = lower.spec(root, meta);
    spec.files = positions.map(|p| p.files.clone()).unwrap_or_default();
    spec.paths = std::mem::take(&mut lower.paths);
    spec.exprs = std::mem::take(&mut lower.exprs);
    Lowered { spec }
}

pub(super) struct Lower<'d> {
    d: &'d Document,
    positions: Option<&'d Positions>,
    /// The spelling the document is written in.
    surface: Surface,
    paths: Paths,
    exprs: Vec<Expr>,
}

/// The path segment of a written registry keyword, such as an operation.
fn keyword(name: &str) -> Segment {
    Segment::Key(name.into())
}

/// Whether a 1.0 input's path names an ODM XML file, which selects the ODM
/// profile (P-ODMIN-1); the extension is matched as `SourceProfile` matches.
fn odm_path(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .is_some_and(|(stem, extension)| !stem.is_empty() && extension.eq_ignore_ascii_case("xml"))
}

/// `{literal: null}`, the one spelling of the missing value a handler, flag,
/// mapping entry or function argument substitutes (COLUMN-10).
fn missing_literal(d: &Document, id: usize) -> bool {
    matches!(&d.nodes()[id], N::Mapping(items) if matches!(
        items.as_slice(),
        &[(key, value)] if matches!(&d.nodes()[key], N::Text(key) if key == "literal")
            && matches!(d.nodes()[value], N::Null)
    ))
}

pub(super) fn column_type(text: &str) -> Option<ColumnType> {
    Some(match text {
        "str" => ColumnType::Str,
        "int" => ColumnType::Int,
        "float" => ColumnType::Float,
        "date" => ColumnType::Date,
        "datetime" => ColumnType::DateTime,
        _ => return None,
    })
}

impl<'d> Lower<'d> {
    fn node(&self, id: usize) -> &'d N {
        &self.d.nodes()[id]
    }
    /// A field that is written and not null.
    fn field(&self, id: usize, name: &str) -> Option<usize> {
        self.d
            .field(id, name)
            .filter(|&id| !matches!(self.node(id), N::Null))
    }
    fn text(&self, id: usize) -> Option<&'d str> {
        match self.node(id) {
            N::Text(text) => Some(text),
            _ => None,
        }
    }
    fn text_of(&self, id: usize, name: &str) -> Option<&'d str> {
        self.field(id, name).and_then(|id| self.text(id))
    }
    fn items(&self, id: usize) -> &'d [usize] {
        match self.node(id) {
            N::Sequence(items) => items,
            _ => &[],
        }
    }
    fn entries(&self, id: usize) -> &'d [(usize, usize)] {
        match self.node(id) {
            N::Mapping(entries) => entries,
            _ => &[],
        }
    }
    /// Which of `fields` `owner` writes with a value, in written order.
    pub(super) fn written(&self, owner: usize, fields: &[&'static str]) -> Written {
        Written(
            self.entries(owner)
                .iter()
                .filter(|&&(_, value)| !matches!(self.node(value), N::Null))
                .filter_map(|&(key, _)| {
                    let key = self.text(key)?;
                    fields.iter().copied().find(|&field| field == key)
                })
                .collect(),
        )
    }
    fn boolean(&self, id: usize, name: &str, default: bool) -> bool {
        match self.field(id, name).map(|id| self.node(id)) {
            Some(N::Boolean(value)) => *value,
            _ => default,
        }
    }
    fn span(&self, node: Option<usize>, inherited: Span) -> Span {
        node.and_then(|node| self.positions?.nodes.get(node).copied().flatten())
            .unwrap_or(inherited)
    }
    /// The meta of the node `segment` names inside `parent`.
    pub(super) fn meta(&mut self, parent: Meta, segment: Segment, node: Option<usize>) -> Meta {
        Meta {
            path: self.paths.child(parent.path, segment),
            span: self.span(node, parent.span),
        }
    }
    fn field_meta(&mut self, parent: Meta, name: &'static str, node: usize) -> Meta {
        self.meta(parent, Segment::Field(name), Some(node))
    }
    /// `meta`'s path at the position of `node`, the deepest node written
    /// there: a 1.0 expression field's text or scalar, which a composed
    /// wrapper above it may hold from another layer (P-ISSUE-1).
    pub(super) fn written_at(&self, meta: Meta, node: usize) -> Meta {
        Meta {
            path: meta.path,
            span: self.span(Some(node), meta.span),
        }
    }
    /// A node's path as its surface's diagnostics cite it.
    pub(super) fn render(&self, meta: Meta) -> String {
        self.paths.render(meta.path, self.notation())
    }
    fn notation(&self) -> Notation {
        match self.surface {
            Surface::V0_5 => Notation::V0_5,
            Surface::V1 { .. } => Notation::V1,
        }
    }
    /// Whether the document is written in the 1.0 spelling.
    pub(super) fn v1(&self) -> bool {
        matches!(self.surface, Surface::V1 { .. })
    }
    pub(super) fn push(&mut self, kind: ExprKind, meta: Meta) -> ExprId {
        let id = ExprId(u32::try_from(self.exprs.len()).expect("a spec's expressions fit u32"));
        self.exprs.push(Expr { kind, meta });
        id
    }
    /// Reserve a parent before its children, so that ids follow pre-order.
    pub(super) fn reserve(&mut self, meta: Meta) -> ExprId {
        self.push(ExprKind::Unsupported(String::new()), meta)
    }
    pub(super) fn fill(&mut self, id: ExprId, kind: ExprKind) -> ExprId {
        self.exprs[id.index()].kind = kind;
        id
    }
    fn named_at(&self, name: &str, meta: Meta) -> Named {
        Named {
            name: name.into(),
            meta,
        }
    }
    /// An identifier field; a required one the model guarantees.
    fn named(&mut self, parent: Meta, owner: usize, name: &'static str) -> Option<Named> {
        let node = self.field(owner, name)?;
        let text = self.text(node)?;
        let meta = self.field_meta(parent, name, node);
        Some(self.named_at(text, meta))
    }
    fn text_field(&mut self, parent: Meta, owner: usize, name: &'static str) -> Option<Text> {
        let node = self.field(owner, name)?;
        let text = self.text(node)?;
        Some(Text {
            text: text.into(),
            meta: self.field_meta(parent, name, node),
        })
    }
    /// A field the model requires; an absent one is empty text at the parent.
    fn required_text(&mut self, parent: Meta, owner: usize, name: &'static str) -> Text {
        self.text_field(parent, owner, name).unwrap_or(Text {
            text: String::new(),
            meta: parent,
        })
    }
    fn texts(&mut self, parent: Meta, owner: usize, name: &'static str) -> Vec<Text> {
        let Some(list) = self.field(owner, name) else {
            return Vec::new();
        };
        let meta = self.field_meta(parent, name, list);
        let mut result = Vec::new();
        for (index, &item) in self.items(list).iter().enumerate() {
            if let Some(text) = self.text(item) {
                let meta = self.meta(meta, Segment::Index(index as u32), Some(item));
                result.push(Text {
                    text: text.into(),
                    meta,
                });
            }
        }
        result
    }
    fn names(&mut self, parent: Meta, owner: usize, name: &'static str) -> Vec<Named> {
        self.texts(parent, owner, name)
            .into_iter()
            .map(|text| Named {
                name: text.text,
                meta: text.meta,
            })
            .collect()
    }
    /// A scalar as written; `{literal: null}` is the missing value.
    pub(super) fn literal(&self, node: usize) -> Literal {
        match self.node(node) {
            N::Null => Literal::Missing,
            N::Boolean(value) => Literal::Bool(*value),
            N::Integer(text) => Literal::Int(text.clone()),
            N::Float(value) => Literal::Float(*value),
            N::Text(text) => Literal::Text(text.clone()),
            N::Mapping(_) if missing_literal(self.d, node) => Literal::Missing,
            _ => Literal::Unsupported,
        }
    }
    /// A handler or substitute value: written, even as null, or omitted.
    pub(super) fn constant(
        &mut self,
        parent: Meta,
        owner: usize,
        name: &'static str,
    ) -> Option<Constant> {
        let node = self.d.field(owner, name)?;
        Some(Constant {
            value: self.literal(node),
            meta: self.field_meta(parent, name, node),
        })
    }
    /// A bound or a count: written and not null.
    fn bound(&mut self, parent: Meta, owner: usize, name: &'static str) -> Option<Constant> {
        self.field(owner, name)?;
        self.constant(parent, owner, name)
    }
    fn constant_at(&mut self, node: usize, meta: Meta) -> Constant {
        Constant {
            value: self.literal(node),
            meta,
        }
    }
    fn path_base(&self, owner: usize) -> PathBase {
        match self.text_of(owner, "relative_to") {
            Some("entry") => PathBase::Entry,
            _ => PathBase::Declaring,
        }
    }
    fn severity(&self, owner: usize) -> Severity {
        match self.text_of(owner, "severity") {
            Some("warning") => Severity::Warning,
            _ => Severity::Error,
        }
    }

    fn spec(&mut self, root: usize, meta: Meta) -> Spec {
        // A 1.0 resolved specification records the chain's version in
        // `yamaa` (P-VER-1).
        let header_field = if self.v1() { "yamaa" } else { "schema_version" };
        let version = self.required_text(meta, root, header_field);
        let header = Header {
            surface: self.surface,
            meta: version.meta,
            version,
        };
        // `model` is stored unchecked; P1-10 checks it (P-MODEL-1).
        let model = self.field(root, "model").and_then(|node| {
            let kind = match self.text(node)? {
                "SDTM" => ModelKind::Sdtm,
                "SEND" => ModelKind::Send,
                "ADaM" => ModelKind::Adam,
                _ => return None,
            };
            Some(Model {
                kind,
                meta: self.field_meta(meta, "model", node),
            })
        });
        let domain = self.named(meta, root, "domain").unwrap_or(Named {
            name: String::new(),
            meta,
        });
        let keys = self.names(meta, root, "keys");
        let inputs = self.inputs(root, meta);
        let columns = self.columns(root, meta);
        let rows = self.rows(root, meta);
        let drivers = rows
            .templates
            .iter()
            .filter_map(|template| template.driver.as_ref().map(|driver| driver.name.clone()))
            .collect::<BTreeSet<_>>();
        let intermediates = self.intermediates(root, meta, &drivers);
        let checks = self.dataset_checks(root, meta);
        let output = self.output(root, meta);
        let submission = self
            .field(root, "submission")
            .map(|node| self.dataset_submission(node, meta));
        let metadata = self.metadata(root, meta);
        Spec {
            header,
            model,
            domain,
            keys,
            inputs,
            rows,
            intermediates,
            columns,
            checks,
            output,
            submission,
            metadata,
            exprs: Vec::new(),
            paths: Paths::new(),
            files: Vec::new(),
            written: self.written(root, &["parent"]),
            meta,
        }
    }

    fn inputs(&mut self, root: usize, meta: Meta) -> Vec<Input> {
        let Some(node) = self.field(root, "input") else {
            return Vec::new();
        };
        let list = self.field_meta(meta, "input", node);
        let mut inputs = Vec::new();
        for &(key, source) in self.entries(node) {
            let Some(name) = self.text(key) else { continue };
            let meta = self.meta(list, Segment::Key(name.into()), Some(source));
            let path = self.required_text(meta, source, "path");
            let profile = crate::specification::SourceProfile::from_path(&path.text)
                .map(|profile| match profile {
                    crate::specification::SourceProfile::Csv => Profile::Csv,
                    crate::specification::SourceProfile::Parquet => Profile::Parquet,
                    crate::specification::SourceProfile::Xport => Profile::Xport,
                    crate::specification::SourceProfile::Sas7bdat => Profile::Sas7bdat,
                })
                .or_else(|| (self.v1() && odm_path(&path.text)).then_some(Profile::OdmXml));
            let mut types = Vec::new();
            if let Some(declared) = self.field(source, "types") {
                let types_meta = self.field_meta(meta, "types", declared);
                for &(field, node) in self.entries(declared) {
                    if let (Some(field), Some(kind)) =
                        (self.text(field), self.text(node).and_then(column_type))
                    {
                        let meta = self.meta(types_meta, Segment::Key(field.into()), Some(node));
                        types.push((self.named_at(field, meta), kind));
                    }
                }
            }
            let source_node = InputSource {
                relative_to: self.path_base(source),
                profile,
                types,
                encoding: self.text_field(meta, source, "encoding"),
                schema: self.text_field(meta, source, "schema"),
                empty_string: match self.text_of(source, "empty_string") {
                    Some("present") => EmptyString::Present,
                    _ => EmptyString::Missing,
                },
                ordinal: self.named(meta, source, "ordinal"),
                written: self.written(source, &["types"]),
                path,
            };
            // A 1.0 input declares the key its implicit joins and keyless
            // qualified aggregates match on (P-JOIN-1); P1-6 binds it.
            let key = if !self.v1() {
                KeyDecl::Inferred
            } else if self.field(source, "key").is_some() {
                KeyDecl::Declared(self.names(meta, source, "key"))
            } else {
                KeyDecl::Undeclared
            };
            inputs.push(Input {
                name: self.named_at(name, meta),
                source: source_node,
                key,
                require_match: self.boolean(source, "require_match", false),
                meta,
            });
        }
        inputs
    }

    fn columns(&mut self, root: usize, meta: Meta) -> Vec<Column> {
        let Some(node) = self.field(root, "columns") else {
            return Vec::new();
        };
        let list = self.field_meta(meta, "columns", node);
        let mut columns = Vec::new();
        for (index, &column) in self.items(node).iter().enumerate() {
            let name = self.text_of(column, "name").unwrap_or_default();
            let meta = self.meta(
                list,
                Segment::Member {
                    index: index as u32,
                    key: name.into(),
                },
                Some(column),
            );
            let declared = self.named(meta, column, "name").unwrap_or(Named {
                name: name.into(),
                meta,
            });
            // In 1.0 a written `derivation: null` derives missing (LAYER-9).
            let written = if self.v1() {
                self.d.field(column, "derivation")
            } else {
                self.field(column, "derivation")
            };
            let derivation = written.map(|node| {
                let meta = self.field_meta(meta, "derivation", node);
                self.derivation(node, meta)
            });
            let checks = self.column_checks(column, meta);
            let submission = self
                .field(column, "submission")
                .map(|node| self.column_submission(node, meta));
            columns.push(Column {
                name: declared,
                kind: self
                    .text_of(column, "type")
                    .and_then(column_type)
                    .unwrap_or(ColumnType::Str),
                label: self.text_field(meta, column, "label"),
                derivation,
                origin: ColumnOrigin::Declared,
                checks,
                submission,
                metadata: self.metadata(column, meta),
                meta,
            });
        }
        columns
    }

    /// A handled expression: `{value: E, unconvertible: V}`.
    pub(super) fn derivation(&mut self, node: usize, meta: Meta) -> Derivation {
        let expr = match self.d.field(node, "value") {
            Some(value) => self.expression(value, meta),
            // A 1.0 expression written bare, as a document admitted without
            // normalization holds it.
            None if self.v1() => self.expression(node, meta),
            None => self.push(ExprKind::Unsupported("derivation".into()), meta),
        };
        Derivation {
            expr,
            unconvertible: self.constant(meta, node, "unconvertible"),
            meta,
        }
    }

    fn rows(&mut self, root: usize, meta: Meta) -> Rows {
        let base = self.named(meta, root, "base");
        let filter = self.field(root, "filter").map(|node| {
            let meta = self.field_meta(meta, "filter", node);
            self.predicate(node, meta)
        });
        let mut templates = Vec::new();
        if let Some(node) = self.field(root, "rows") {
            let list = self.field_meta(meta, "rows", node);
            for (index, &row) in self.items(node).iter().enumerate() {
                templates.push(self.template(row, index, list));
            }
        }
        Rows {
            base,
            filter,
            templates,
        }
    }

    fn template(&mut self, row: usize, index: usize, list: Meta) -> Template {
        let id = self.text_of(row, "id").unwrap_or_default();
        let meta = self.meta(
            list,
            Segment::Member {
                index: index as u32,
                key: id.into(),
            },
            Some(row),
        );
        let declared = self.named(meta, row, "id").unwrap_or(Named {
            name: id.into(),
            meta,
        });
        let driver = self.named(meta, row, "dataset");
        let group_by = self.field(row, "group_by").map(|node| {
            let meta = self.field_meta(meta, "group_by", node);
            self.references(node, meta)
        });
        let filter = self.field(row, "filter").map(|node| {
            let meta = self.field_meta(meta, "filter", node);
            self.predicate(node, meta)
        });
        let mut overrides = Vec::new();
        if let Some(node) = self.field(row, "derivations") {
            let entries_meta = self.field_meta(meta, "derivations", node);
            for &(key, value) in self.entries(node) {
                let Some(name) = self.text(key) else { continue };
                let meta = self.meta(entries_meta, Segment::Key(name.into()), Some(value));
                let derivation = self.derivation(value, meta);
                overrides.push((self.named_at(name, meta), derivation));
            }
        }
        Template {
            id: declared,
            driver,
            group_by,
            filter,
            overrides,
            meta,
        }
    }

    fn intermediates(
        &mut self,
        root: usize,
        meta: Meta,
        drivers: &BTreeSet<String>,
    ) -> Vec<Intermediate> {
        let Some(node) = self.field(root, "intermediates") else {
            return Vec::new();
        };
        let list = self.field_meta(meta, "intermediates", node);
        let mut intermediates = Vec::new();
        for (index, &item) in self.items(node).iter().enumerate() {
            let id = self.text_of(item, "id").unwrap_or_default();
            let meta = self.meta(
                list,
                Segment::Member {
                    index: index as u32,
                    key: id.into(),
                },
                Some(item),
            );
            let declared = self.named(meta, item, "id").unwrap_or(Named {
                name: id.into(),
                meta,
            });
            let dataset = self.named(meta, item, "dataset").unwrap_or(Named {
                name: String::new(),
                meta,
            });
            // A 0.5 lookup without `key` infers the applicable keys at
            // binding; a driver of row templates has no key (ROW-2, D23-R1).
            // A 1.0 intermediate states its own key (D23), and one that does
            // not keeps none here: P1-6 refuses it unless it drives rows.
            let key = match self.field(item, "key") {
                Some(node) => {
                    let meta = self.field_meta(meta, "key", node);
                    Some(self.join_key(node, meta))
                }
                None => (!self.v1() && !drivers.contains(id)).then_some(JoinKey::Applicable),
            };
            let between = self.field(item, "between").map(|node| {
                let meta = self.field_meta(meta, "between", node);
                self.between(node, meta)
            });
            let filter = self.field(item, "filter").map(|node| {
                let meta = self.field_meta(meta, "filter", node);
                self.predicate(node, meta)
            });
            let order_by = self.order_by(item, "order_by", meta);
            let keep = self.keep(item);
            let columns = self
                .field(item, "columns")
                .map(|_| self.names(meta, item, "columns"));
            // The absence answer is `no_match` in 0.5 and `absent` in 1.0
            // (P-HND-1).
            let absent = if self.v1() {
                self.constant(meta, item, "absent")
            } else {
                self.constant(meta, item, "no_match")
            };
            let mut bindings = Vec::new();
            if let Some(node) = self.field(item, "derivations") {
                let entries_meta = self.field_meta(meta, "derivations", node);
                for &(key, entry) in self.entries(node) {
                    if let Some(name) = self.text(key) {
                        bindings.push(self.intermediate_binding(name, entry, entries_meta));
                    }
                }
            }
            let mut checks = Vec::new();
            if let Some(node) = self.field(item, "verifications") {
                let checks_meta = self.field_meta(meta, "verifications", node);
                for (position, &check) in self.items(node).iter().enumerate() {
                    let Some(unique) = self.field(check, "unique") else {
                        continue;
                    };
                    let entry =
                        self.meta(checks_meta, Segment::Index(position as u32), Some(check));
                    let meta = self.field_meta(entry, "unique", unique);
                    checks.push(UniqueCheck {
                        columns: self.names(meta, unique, "columns"),
                        id: self.text_field(meta, unique, "id"),
                        meta,
                    });
                }
            }
            intermediates.push(Intermediate {
                id: declared,
                dataset,
                key,
                between,
                filter,
                order_by,
                keep,
                columns,
                absent,
                require_match: self.boolean(item, "require_match", false),
                bindings,
                checks,
                written: self.written(
                    item,
                    &[
                        "key",
                        "between",
                        "filter",
                        "order_by",
                        "keep",
                        "columns",
                        "derivations",
                        "verifications",
                        if self.v1() { "absent" } else { "no_match" },
                        "require_match",
                    ],
                ),
                meta,
            });
        }
        intermediates
    }

    /// An intermediate derivation: `{type, derivation, unconvertible}`, or an
    /// untyped expression, which 0.5 writes `{value: E}` and 1.0 bare
    /// (P-BIND-2).
    fn intermediate_binding(&mut self, name: &str, entry: usize, list: Meta) -> Binding {
        let meta = self.meta(list, Segment::Key(name.into()), Some(entry));
        let (declared, derivation) = match self.d.field(entry, "derivation") {
            Some(node) => {
                let derivation_meta = self.field_meta(meta, "derivation", node);
                let expr = match self.d.field(node, "value") {
                    Some(value) => self.expression(value, derivation_meta),
                    None => self.expression(node, derivation_meta),
                };
                (
                    self.text_of(entry, "type").and_then(column_type),
                    Derivation {
                        expr,
                        unconvertible: self.constant(meta, entry, "unconvertible"),
                        meta: derivation_meta,
                    },
                )
            }
            None => (None, self.derivation(entry, meta)),
        };
        Binding {
            name: self.named_at(name, meta),
            declared,
            derivation,
            meta,
        }
    }

    fn column_checks(&mut self, column: usize, meta: Meta) -> Vec<ColumnCheck> {
        let Some(node) = self.field(column, "verifications") else {
            return Vec::new();
        };
        let list = self.field_meta(meta, "verifications", node);
        let mut checks = Vec::new();
        for (index, &check) in self.items(node).iter().enumerate() {
            let entry = self.meta(list, Segment::Index(index as u32), Some(check));
            let (operation, payload) = match (self.entries(check), self.text(check)) {
                (&[(key, payload)], _) => (self.text(key).unwrap_or_default(), payload),
                // 1.0 writes a check that declares no field as its bare
                // keyword, `- not_missing` (P-HND-4); it keeps the keyword's
                // path.
                (_, Some(keyword)) if self.v1() => (keyword, check),
                _ => ("", check),
            };
            let meta = self.meta(entry, keyword(operation), Some(payload));
            let kind = match operation {
                "not_missing" => ColumnCheckKind::NotMissing,
                "allowed_values" => {
                    let mut values = Vec::new();
                    if let Some(list) = self.field(payload, "values") {
                        let list_meta = self.field_meta(meta, "values", list);
                        for (position, &value) in self.items(list).iter().enumerate() {
                            let meta =
                                self.meta(list_meta, Segment::Index(position as u32), Some(value));
                            values.push(self.constant_at(value, meta));
                        }
                    }
                    ColumnCheckKind::AllowedValues(values)
                }
                "range" => ColumnCheckKind::Range {
                    min: self.bound(meta, payload, "min"),
                    max: self.bound(meta, payload, "max"),
                },
                "max_length" => match self.constant(meta, payload, "max") {
                    Some(max) => ColumnCheckKind::MaxLength(max),
                    None => ColumnCheckKind::Unsupported(operation.into()),
                },
                "matches" => match self.text_field(meta, payload, "pattern") {
                    Some(pattern) => ColumnCheckKind::Matches(pattern),
                    None => ColumnCheckKind::Unsupported(operation.into()),
                },
                _ => ColumnCheckKind::Unsupported(operation.into()),
            };
            checks.push(ColumnCheck {
                kind,
                id: self.text_field(meta, payload, "id"),
                severity: self.severity(payload),
                meta,
            });
        }
        checks
    }

    fn dataset_checks(&mut self, root: usize, meta: Meta) -> Vec<DatasetCheck> {
        let Some(node) = self.field(root, "verifications") else {
            return Vec::new();
        };
        let list = self.field_meta(meta, "verifications", node);
        let mut checks = Vec::new();
        for (index, &check) in self.items(node).iter().enumerate() {
            let entry = self.meta(list, Segment::Index(index as u32), Some(check));
            let (operation, payload) = match self.entries(check) {
                &[(key, payload)] => (self.text(key).unwrap_or_default(), payload),
                _ => ("", check),
            };
            let meta = self.meta(entry, keyword(operation), Some(payload));
            let kind = match operation {
                "unique" | "all_or_none" => {
                    // `unique` may be written as its bare column list.
                    let columns = if matches!(self.node(payload), N::Sequence(_)) {
                        let mut columns = Vec::new();
                        for (position, &column) in self.items(payload).iter().enumerate() {
                            if let Some(name) = self.text(column) {
                                let meta =
                                    self.meta(meta, Segment::Index(position as u32), Some(column));
                                columns.push(self.named_at(name, meta));
                            }
                        }
                        columns
                    } else {
                        self.names(meta, payload, "columns")
                    };
                    if operation == "unique" {
                        DatasetCheckKind::Unique { columns }
                    } else {
                        DatasetCheckKind::AllOrNone { columns }
                    }
                }
                "assert" => match self.field(payload, "require") {
                    Some(require) => {
                        let when = self.field(payload, "when").map(|node| {
                            let meta = self.field_meta(meta, "when", node);
                            self.predicate(node, meta)
                        });
                        let require_meta = self.field_meta(meta, "require", require);
                        let require = self.predicate(require, require_meta);
                        DatasetCheckKind::Assert { when, require }
                    }
                    None => DatasetCheckKind::Unsupported(operation.into()),
                },
                "row_count" => {
                    let group_by = self
                        .field(payload, "group_by")
                        .map(|_| self.names(meta, payload, "group_by"));
                    let filter = self.field(payload, "filter").map(|node| {
                        let meta = self.field_meta(meta, "filter", node);
                        self.predicate(node, meta)
                    });
                    let when = self.field(payload, "when").map(|node| {
                        let meta = self.field_meta(meta, "when", node);
                        self.predicate(node, meta)
                    });
                    DatasetCheckKind::RowCount(Box::new(RowCount {
                        group_by,
                        filter,
                        when,
                        min: self.bound(meta, payload, "min"),
                        max: self.bound(meta, payload, "max"),
                        min_fraction: self.bound(meta, payload, "min_fraction"),
                        max_fraction: self.bound(meta, payload, "max_fraction"),
                    }))
                }
                _ => DatasetCheckKind::Unsupported(operation.into()),
            };
            let id = if matches!(self.node(payload), N::Mapping(_)) {
                self.text_field(meta, payload, "id")
            } else {
                None
            };
            checks.push(DatasetCheck {
                kind,
                id,
                severity: self.severity(payload),
                meta,
            });
        }
        checks
    }

    fn output(&mut self, root: usize, meta: Meta) -> Output {
        let Some(node) = self.field(root, "output") else {
            return Output {
                path: Text {
                    text: String::new(),
                    meta,
                },
                relative_to: PathBase::Declaring,
                decimals: None,
                columns: Vec::new(),
                report: None,
                order_by: Vec::new(),
                meta,
            };
        };
        let meta = self.field_meta(meta, "output", node);
        Output {
            path: self.required_text(meta, node, "path"),
            relative_to: self.path_base(node),
            decimals: self.bound(meta, node, "decimals"),
            columns: self.names(meta, node, "columns"),
            report: self.text_field(meta, node, "report"),
            order_by: self.order_by(node, "order_by", meta),
            meta,
        }
    }

    fn metadata(&mut self, owner: usize, parent: Meta) -> Vec<(Named, Text)> {
        let Some(node) = self.field(owner, "metadata") else {
            return Vec::new();
        };
        let list = self.field_meta(parent, "metadata", node);
        let mut entries = Vec::new();
        for &(key, node) in self.entries(node) {
            if let (Some(key), Some(value)) = (self.text(key), self.text(node)) {
                let meta = self.meta(list, Segment::Key(key.into()), Some(node));
                entries.push((
                    self.named_at(key, meta),
                    Text {
                        text: value.into(),
                        meta,
                    },
                ));
            }
        }
        entries
    }

    fn dataset_submission(&mut self, node: usize, parent: Meta) -> DatasetSubmission {
        let meta = self.field_meta(parent, "submission", node);
        DatasetSubmission {
            label: self.required_text(meta, node, "label"),
            class: self.required_text(meta, node, "class"),
            subclass: self.text_field(meta, node, "subclass"),
            structure: self.required_text(meta, node, "structure"),
            repeating: self.boolean(node, "repeating", false),
            reference_data: self.boolean(node, "reference_data", false),
            has_no_data: self.boolean(node, "has_no_data", false),
            domain: self.named(meta, node, "domain"),
            comment: self.comment(node, meta),
            // The section standard a 1.0 spec selects (P-STD-1); P1-13
            // checks it.
            standard: self.named(meta, node, "standard"),
            meta,
        }
    }

    fn column_submission(&mut self, node: usize, parent: Meta) -> ColumnSubmission {
        let meta = self.field_meta(parent, "submission", node);
        let metadata = self.submission_metadata(node, meta);
        let mut value_level = Vec::new();
        if let Some(list) = self.field(node, "value_level") {
            let list_meta = self.field_meta(meta, "value_level", list);
            for (index, &entry) in self.items(list).iter().enumerate() {
                let id = self.text_of(entry, "id").unwrap_or_default();
                let meta = self.meta(
                    list_meta,
                    Segment::Member {
                        index: index as u32,
                        key: id.into(),
                    },
                    Some(entry),
                );
                let declared = self.named(meta, entry, "id").unwrap_or(Named {
                    name: id.into(),
                    meta,
                });
                let mut conditions = Vec::new();
                if let Some(clause) = self.field(entry, "where") {
                    let clause_meta = self.field_meta(meta, "where", clause);
                    for &(key, values) in self.entries(clause) {
                        let Some(column) = self.text(key) else {
                            continue;
                        };
                        let meta =
                            self.meta(clause_meta, Segment::Key(column.into()), Some(values));
                        // 1.0 holds the list in its comparison class, beside
                        // an optional NE or NOTIN (P-VLM-1). Each selects
                        // none of its values: NE is `NotIn` with its one
                        // value, as EQ is `In` with one, and NOTIN is
                        // `NotIn`. The switch refuses both until P1-14 builds
                        // them (P1-9, D66). A class holding only `values` is
                        // the expanded list written bare (SCHEMA-15), whose
                        // values keep their written paths.
                        let (comparator, list, list_meta) = match self.field(values, "values") {
                            Some(list) => match self.text_of(values, "comparator") {
                                Some(_) => (
                                    Comparator::NotIn,
                                    list,
                                    self.field_meta(meta, "values", list),
                                ),
                                None => (Comparator::In, list, meta),
                            },
                            None => (Comparator::In, values, meta),
                        };
                        let mut listed = Vec::new();
                        for (position, &value) in self.items(list).iter().enumerate() {
                            if let Some(text) = self.text(value) {
                                let meta = self.meta(
                                    list_meta,
                                    Segment::Index(position as u32),
                                    Some(value),
                                );
                                listed.push(Text {
                                    text: text.into(),
                                    meta,
                                });
                            }
                        }
                        conditions.push(WhereClause {
                            column: self.named_at(column, meta),
                            comparator,
                            values: listed,
                            meta,
                        });
                    }
                }
                value_level.push(ValueLevel {
                    id: declared,
                    conditions,
                    label: self.text_field(meta, entry, "label"),
                    metadata: self.submission_metadata(entry, meta),
                    meta,
                });
            }
        }
        ColumnSubmission {
            metadata,
            value_level,
            meta,
        }
    }

    fn submission_metadata(&mut self, node: usize, meta: Meta) -> SubmissionMetadata {
        // 0.5 declares one `origin`; 1.0 declares `origins`, always a list
        // (P-ORIGIN-1). Both lower to the list.
        let mut origins = Vec::new();
        if self.v1() {
            if let Some(list) = self.field(node, "origins") {
                let list_meta = self.field_meta(meta, "origins", list);
                for (index, &origin) in self.items(list).iter().enumerate() {
                    let meta = self.meta(list_meta, Segment::Index(index as u32), Some(origin));
                    origins.push(self.origin(origin, meta));
                }
            }
        } else if let Some(origin) = self.field(node, "origin") {
            let meta = self.field_meta(meta, "origin", origin);
            origins.push(self.origin(origin, meta));
        }
        let method = self.field(node, "method").map(|method| {
            let meta = self.field_meta(meta, "method", method);
            if let Some(text) = self.text(method) {
                return Method {
                    name: None,
                    kind: None,
                    description: Text {
                        text: text.into(),
                        meta,
                    },
                    expression: None,
                    documents: Vec::new(),
                    meta,
                };
            }
            Method {
                name: self.text_field(meta, method, "name"),
                kind: self.text_field(meta, method, "type"),
                description: self.required_text(meta, method, "description"),
                expression: self.field(method, "expression").map(|expression| {
                    let meta = self.field_meta(meta, "expression", expression);
                    FormalExpression {
                        context: self.required_text(meta, expression, "context"),
                        code: self.required_text(meta, expression, "code"),
                        meta,
                    }
                }),
                documents: self.documents(method, meta),
                meta,
            }
        });
        SubmissionMetadata {
            core: self.text_field(meta, node, "core"),
            mandatory: self
                .field(node, "mandatory")
                .map(|_| self.boolean(node, "mandatory", false)),
            role: self.text_field(meta, node, "role"),
            data_type: self.text_field(meta, node, "data_type"),
            length: self.bound(meta, node, "length"),
            significant_digits: self.bound(meta, node, "significant_digits"),
            display_format: self.text_field(meta, node, "display_format"),
            codelist: self.named(meta, node, "codelist"),
            inventory_vocabulary: self
                .field(node, "inventory_vocabulary")
                .map(|_| self.boolean(node, "inventory_vocabulary", false)),
            origins,
            method,
            comment: self.comment(node, meta),
        }
    }

    fn origin(&mut self, origin: usize, meta: Meta) -> Origin {
        Origin {
            kind: self.required_text(meta, origin, "type"),
            source: self.text_field(meta, origin, "source"),
            description: self.text_field(meta, origin, "description"),
            documents: self.documents(origin, meta),
            meta,
        }
    }

    fn comment(&mut self, owner: usize, parent: Meta) -> Option<Comment> {
        let node = self.field(owner, "comment")?;
        let meta = self.field_meta(parent, "comment", node);
        Some(match self.text(node) {
            Some(text) => Comment {
                text: Text {
                    text: text.into(),
                    meta,
                },
                documents: Vec::new(),
                meta,
            },
            None => Comment {
                text: self.required_text(meta, node, "text"),
                documents: self.documents(node, meta),
                meta,
            },
        })
    }

    fn documents(&mut self, owner: usize, parent: Meta) -> Vec<DocumentRef> {
        let Some(list) = self.field(owner, "documents") else {
            return Vec::new();
        };
        let list_meta = self.field_meta(parent, "documents", list);
        let mut documents = Vec::new();
        for (index, &entry) in self.items(list).iter().enumerate() {
            let meta = self.meta(list_meta, Segment::Index(index as u32), Some(entry));
            documents.push(match self.text(entry) {
                Some(document) => DocumentRef {
                    document: self.named_at(document, meta),
                    pages: None,
                    meta,
                },
                None => DocumentRef {
                    document: self.named(meta, entry, "document").unwrap_or(Named {
                        name: String::new(),
                        meta,
                    }),
                    pages: self.field(entry, "pages").map(|pages| {
                        let meta = self.field_meta(meta, "pages", pages);
                        PageRef {
                            kind: self.required_text(meta, pages, "type"),
                            refs: self.required_text(meta, pages, "refs"),
                            title: self.text_field(meta, pages, "title"),
                            meta,
                        }
                    }),
                    meta,
                },
            });
        }
        documents
    }
}
