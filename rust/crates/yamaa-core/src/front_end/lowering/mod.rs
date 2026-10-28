//! The construct lowering of a resolved 1.x specification into the AST.
//!
//! The 1.0 front end ([`crate::front_end::v1::spec`]) enters it with the
//! version the specification declares (`lower_specification`). Lowering
//! runs once per specification, after composition and pruning, on the
//! resolved document. Pruning lowers the composed document before that, only
//! to find what each declaration reads.
//!
//! Lowering is total over every document the schema admits. An operation
//! outside the language lowers to [`ExprKind::Unsupported`], which
//! preparation refuses.
//!
//! What a project's own files write alike in both surfaces lowers in
//! [`project`], and a project's study and sections in [`sections`].

mod expressions;
pub mod project;
pub mod sections;

use crate::ast::{
    Binding, Column, ColumnCheck, ColumnCheckKind, ColumnOrigin, ColumnSubmission, Comment,
    Comparator, Constant, DatasetCheck, DatasetCheckKind, DatasetSubmission, Derivation,
    DocumentRef, EmptyString, Expr, ExprId, ExprKind, FileRef, FormalExpression, Header, Input,
    InputSource, Intermediate, KeyDecl, Literal, Meta, Method, Model, Named, Origin, Output,
    PageRef, PathBase, Paths, Profile, RowCount, Rows, Segment, Severity, Span, Spec,
    SubmissionMetadata, Template, Text, UniqueCheck, ValueLevel, WhereClause, Written,
};
use crate::cdisc_vocabulary::ModelKind;
use crate::schema::{Document, DocumentNode as N};
use crate::value::ColumnType;

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

/// Lower a resolved specification. Without positions every span is
/// [`Span::UNKNOWN`]. The 1.0 front end enters here from
/// [`crate::front_end::v1::spec::lower`].
pub(crate) fn lower_specification(d: &Document, positions: Option<&Positions>) -> Lowered {
    let mut lower = Lower {
        d,
        positions,
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
    paths: Paths,
    exprs: Vec<Expr>,
}

/// The path segment of a written registry keyword, such as an operation.
fn keyword(name: &str) -> Segment {
    Segment::Key(name.into())
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
    /// Which of `fields` `owner` writes, in written order. A null is the
    /// missing value written in a value or expression field (D18), so
    /// `absent: null` declares an absence answer (LOOKUP-24).
    pub(super) fn written(&self, owner: usize, fields: &[&'static str]) -> Written {
        Written(
            self.entries(owner)
                .iter()
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
    /// A node's path as diagnostics cite it (P-CLR-2).
    pub(super) fn render(&self, meta: Meta) -> String {
        self.paths.render(meta.path)
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
    /// A scalar as written; YAML null is the missing value (D18).
    pub(super) fn literal(&self, node: usize) -> Literal {
        match self.node(node) {
            N::Null => Literal::Missing,
            N::Boolean(value) => Literal::Bool(*value),
            N::Integer(text) => Literal::Int(text.clone()),
            N::Float(value) => Literal::Float(*value),
            N::Text(text) => Literal::Text(text.clone()),
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
        // A resolved specification records the chain's version in `yamaa`
        // (P-VER-1).
        let version = self.required_text(meta, root, "yamaa");
        let header = Header {
            meta: version.meta,
            version,
        };
        // `model` (P-MODEL-1).
        let model = self.field(root, "model").and_then(|node| {
            let kind = ModelKind::from_name(self.text(node)?)?;
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
        let intermediates = self.intermediates(root, meta);
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
            written: self.written(root, &["parent", "include"]),
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
            // `.xml` selects the ODM profile (INPUT-1, P-ODMIN-1), and `.json`
            // and `.ndjson` the Dataset-JSON profiles (DSJSON-1).
            let profile =
                crate::specification::SourceProfile::from_path(&path.text).map(|profile| {
                    use crate::specification::SourceProfile as Source;
                    match profile {
                        Source::Csv => Profile::Csv,
                        Source::Parquet => Profile::Parquet,
                        Source::Xport => Profile::Xport,
                        Source::Sas7bdat => Profile::Sas7bdat,
                        Source::OdmXml => Profile::OdmXml,
                        Source::DatasetJson => Profile::DatasetJson,
                        Source::DatasetNdjson => Profile::DatasetNdjson,
                    }
                });
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
                byte_order_mark: self.boolean(source, "byte_order_mark", false),
                schema: self.text_field(meta, source, "schema"),
                empty_string: match self.text_of(source, "empty_string") {
                    Some("present") => EmptyString::Present,
                    _ => EmptyString::Missing,
                },
                ordinal: self.named(meta, source, "ordinal"),
                written: self.written(source, &["types", "byte_order_mark"]),
                path,
            };
            // An input declares the key its implicit joins and keyless
            // qualified aggregates match on (P-JOIN-1).
            let key = if self.field(source, "key").is_some() {
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
            // A written `derivation: null` derives missing (LAYER-9).
            let derivation = self.d.field(column, "derivation").map(|node| {
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

    /// A handled expression: `{value: E, unconvertible: V}`, or an expression
    /// written bare, as a document admitted without normalization holds it.
    pub(super) fn derivation(&mut self, node: usize, meta: Meta) -> Derivation {
        let expr = match self.d.field(node, "value") {
            Some(value) => self.expression(value, meta),
            None => self.expression(node, meta),
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

    fn intermediates(&mut self, root: usize, meta: Meta) -> Vec<Intermediate> {
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
            // The right side of a pair relation (LOOKUP-27).
            let inner_join = self.named(meta, item, "inner_join");
            // An intermediate states its own key (D23), and one that does not
            // keeps none here: the front end refuses it unless it drives rows
            // (LOOKUP-3, ROW-2).
            let key = self.field(item, "key").map(|node| {
                let meta = self.field_meta(meta, "key", node);
                self.join_key(node, meta)
            });
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
            let nearest = self.field(item, "nearest").map(|node| {
                let meta = self.field_meta(meta, "nearest", node);
                self.nearest(node, meta)
            });
            let columns = self
                .field(item, "columns")
                .map(|_| self.names(meta, item, "columns"));
            // The absence answer (P-HND-1).
            let absent = self.constant(meta, item, "absent");
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
                        severity: self.severity(unique),
                        meta,
                    });
                }
            }
            intermediates.push(Intermediate {
                id: declared,
                dataset,
                inner_join,
                key,
                between,
                filter,
                order_by,
                keep,
                nearest,
                columns,
                absent,
                require_match: self.boolean(item, "require_match", false),
                bindings,
                checks,
                written: self.written(
                    item,
                    &[
                        "inner_join",
                        "key",
                        "between",
                        "filter",
                        "order_by",
                        "keep",
                        "nearest",
                        "columns",
                        "derivations",
                        "verifications",
                        "absent",
                        "require_match",
                    ],
                ),
                meta,
            });
        }
        intermediates
    }

    /// An intermediate derivation: `{type, derivation, unconvertible}`, or an
    /// untyped expression written bare (P-BIND-2).
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
                // A check that declares no field is written as its bare
                // keyword, `- not_missing` (P-HND-4); it keeps the keyword's
                // path.
                (_, Some(keyword)) => (keyword, check),
                _ => ("", check),
            };
            let meta = self.meta(entry, keyword(operation), Some(payload));
            let kind = match operation {
                // The bare keyword writes neither field.
                "not_missing" => {
                    let when = self.field(payload, "when").map(|node| {
                        let meta = self.field_meta(meta, "when", node);
                        self.predicate(node, meta)
                    });
                    ColumnCheckKind::NotMissing {
                        when,
                        max_fraction: self.bound(meta, payload, "max_fraction"),
                    }
                }
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
            mode: if self.text_of(node, "mode") == Some("historical") {
                crate::specification_submission_document::SubmissionMode::Historical
            } else {
                crate::specification_submission_document::SubmissionMode::Validated
            },
            artifact: self.text_field(meta, node, "artifact"),
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
            non_standard: self.boolean(node, "non_standard", false),
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
                        // an optional NE or NOTIN (P-VLM-1), and the tree
                        // keeps which one was written: SUB-12 gives NE one
                        // value and NOTIN two or more. A class holding only
                        // `values` is the expanded list written bare
                        // (SCHEMA-15), whose values keep their written paths.
                        let (comparator, list, list_meta) = match self.field(values, "values") {
                            Some(list) => match self.text_of(values, "comparator") {
                                Some(written) => (
                                    Comparator::written_as(written),
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
        // `origins` is always a list (P-ORIGIN-1).
        let mut origins = Vec::new();
        if let Some(list) = self.field(node, "origins") {
            let list_meta = self.field_meta(meta, "origins", list);
            for (index, &origin) in self.items(list).iter().enumerate() {
                let meta = self.meta(list_meta, Segment::Index(index as u32), Some(origin));
                origins.push(self.origin(origin, meta));
            }
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
            key_sequence: self.bound(meta, node, "key_sequence"),
            has_no_data: self
                .field(node, "has_no_data")
                .map(|_| self.boolean(node, "has_no_data", false)),
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
