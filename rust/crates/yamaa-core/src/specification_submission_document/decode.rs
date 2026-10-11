//! Complete non-owning admission followed by one owned decode of the same input.
use super::*;
use alloc::{boxed::Box, string::String, vec::Vec};
use FieldName as F;

struct Budget(Limits);
impl Budget {
    fn charge(&mut self, resource: Resource, n: usize) -> Result<(), Cause> {
        let available = match resource {
            Resource::Fields => &mut self.0.fields,
            Resource::Columns => &mut self.0.columns,
            Resource::Declarations => &mut self.0.declarations,
            Resource::Work => &mut self.0.work,
            Resource::Nodes => &mut self.0.nodes,
            Resource::ListItems => &mut self.0.list_items,
            Resource::TextBytes => &mut self.0.text_bytes,
            Resource::EscapedBytes => &mut self.0.escaped_bytes,
            Resource::PathSegments => &mut self.0.path_segments,
            Resource::Findings => &mut self.0.findings,
            Resource::Depth => unreachable!("depth is a maximum"),
        };
        *available = available.checked_sub(n).ok_or(Cause::Limit(resource))?;
        Ok(())
    }
    fn multiply(&mut self, resource: Resource, n: usize, factor: usize) -> Result<(), Cause> {
        self.charge(
            resource,
            n.checked_mul(factor).ok_or(Cause::Limit(resource))?,
        )
    }
    fn depth(&self, depth: usize) -> Result<(), Cause> {
        if depth > self.0.depth.min(64) {
            Err(Cause::Limit(Resource::Depth))
        } else {
            Ok(())
        }
    }
    fn text(&mut self, value: &str) -> Result<(), Cause> {
        self.charge(Resource::TextBytes, value.len())?;
        self.multiply(Resource::EscapedBytes, value.len(), 10)?;
        self.charge(Resource::EscapedBytes, 2)?;
        self.multiply(Resource::Work, value.len(), 128)
    }
    fn mapping(
        &mut self,
        input: &Document,
        fields: &[(usize, usize)],
        depth: usize,
    ) -> Result<(), Cause> {
        self.depth(depth + usize::from(!fields.is_empty()))?;
        self.charge(Resource::Fields, fields.len())?;
        // Includes retained mapping order, comparisons in every closed lookup,
        // validation passes and the final immutable owned pass.
        self.charge(Resource::ListItems, fields.len())?;
        self.multiply(Resource::Work, fields.len(), 256)?;
        for &(key, _) in fields {
            self.charge(Resource::Nodes, 1)?;
            match &input.nodes()[key] {
                N::Text(v) | N::Integer(v) => self.text(v)?,
                _ => {}
            }
        }
        Ok(())
    }
    fn node(&mut self, depth: usize) -> Result<(), Cause> {
        self.depth(depth)?;
        self.charge(Resource::Nodes, 1)?;
        self.charge(Resource::Work, 64)
    }
    fn tree(&mut self, input: &Document, node: usize, depth: usize) -> Result<(), Cause> {
        self.node(depth)?;
        match &input.nodes()[node] {
            N::Text(v) | N::Integer(v) => self.text(v)?,
            N::Sequence(items) => {
                self.charge(Resource::ListItems, items.len())?;
                self.multiply(Resource::Work, items.len(), 64)?;
                for &item in items {
                    self.tree(input, item, depth + 1)?;
                }
            }
            N::Mapping(fields) => {
                self.mapping(input, fields, depth)?;
                for &(_, child) in fields {
                    self.tree(input, child, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn columns(&mut self, input: &Document, node: usize) -> Result<(), Cause> {
        let N::Sequence(items) = &input.nodes()[node] else {
            return self.tree(input, node, 2);
        };
        self.node(2)?;
        self.charge(Resource::Columns, items.len())?;
        self.charge(Resource::ListItems, items.len())?;
        self.multiply(Resource::Work, items.len(), 64)?;
        for &column in items {
            let N::Mapping(fields) = &input.nodes()[column] else {
                self.tree(input, column, 3)?;
                continue;
            };
            self.node(3)?;
            self.mapping(input, fields, 3)?;
            for &(key, value) in fields {
                if let N::Text(text) = &input.nodes()[key] {
                    if matches!(text.as_str(), "name" | "type" | "label" | "submission") {
                        if text == "submission" {
                            self.charge(Resource::Declarations, 1)?;
                        }
                        self.tree(input, value, 4)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn rows(&mut self, input: &Document, node: usize) -> Result<(), Cause> {
        let N::Sequence(items) = &input.nodes()[node] else {
            // Rows without metadata remain ordinary compiler/schema declarations.
            return Ok(());
        };
        self.node(2)?;
        self.charge(Resource::ListItems, items.len())?;
        self.multiply(Resource::Work, items.len(), 64)?;
        for &row in items {
            let N::Mapping(fields) = &input.nodes()[row] else {
                continue;
            };
            self.node(3)?;
            self.mapping(input, fields, 3)?;
            for &(key, value) in fields {
                if let N::Text(text) = &input.nodes()[key] {
                    if text == "id" || text == "submission" {
                        if text == "submission" {
                            if let N::Mapping(entries) = &input.nodes()[value] {
                                self.charge(Resource::Declarations, entries.len())?;
                            }
                        }
                        self.tree(input, value, 4)?;
                    }
                }
            }
        }
        Ok(())
    }
}
const DATASET: &[F] = &[
    F::HasNoData,
    F::Label,
    F::Class,
    F::Subclass,
    F::Structure,
    F::Repeating,
    F::ReferenceData,
    F::Domain,
    F::Comment,
];
const COLUMN: &[F] = &[F::Name, F::Type, F::Label, F::Submission];
const SUBMISSION: &[F] = &[
    F::Core,
    F::Mandatory,
    F::Role,
    F::DataType,
    F::Length,
    F::SignificantDigits,
    F::DisplayFormat,
    F::Codelist,
    F::InventoryVocabulary,
    F::Origin,
    F::Method,
    F::Comment,
];
const ORIGIN: &[F] = &[F::Type, F::Source, F::Description, F::Documents];
const METHOD: &[F] = &[
    F::Name,
    F::Type,
    F::Description,
    F::Expression,
    F::Documents,
];
const COMMENT: &[F] = &[F::Text, F::Documents];
const EXPRESSION: &[F] = &[F::Context, F::Code];
const REFERENCE: &[F] = &[F::Document, F::Pages];
const PAGES: &[F] = &[F::Type, F::Refs, F::Title];
impl Path {
    fn root() -> Self {
        Self {
            steps: [None; 10],
            len: 0,
        }
    }
    fn child(mut self, step: Step) -> Self {
        // A row override's deepest pages field reaches ten segments.
        self.steps[self.len] = Some(step);
        self.len += 1;
        self
    }
    fn field(self, field: F) -> Self {
        self.child(Step::Field(field))
    }
}
/// This admits declarations only. Unselected specification domains remain under
/// ordinary schema/compiler admission. No compilation or host work occurs here.
pub fn decode(input: &Document, root: usize, limits: Limits) -> Result<Declarations, Error<'_>> {
    let refuse = |cause| Error { input, cause };
    let Some(N::Mapping(fields)) = input.nodes().get(root) else {
        return Err(refuse(Cause::Shape { node: root }));
    };
    let mut budget = Budget(limits);
    let count = (|| {
        budget.node(1)?;
        budget.mapping(input, fields, 1)?;
        for &(key, node) in fields {
            if let N::Text(text) = &input.nodes()[key] {
                match text.as_str() {
                    "submission" => {
                        budget.charge(Resource::Declarations, 1)?;
                        budget.tree(input, node, 2)?;
                    }
                    "columns" => budget.columns(input, node)?,
                    "rows" => budget.rows(input, node)?,
                    _ => {}
                }
            }
        }
        let mut pass = Parser {
            input,
            budget: Some(&mut budget),
            model: false,
            findings: Vec::new(),
            count: 0,
        };
        pass.run(root)?;
        Ok(pass.count)
    })()
    .map_err(refuse)?;
    let mut owner = Parser {
        input,
        budget: None,
        model: count == 0,
        findings: Vec::with_capacity(count),
        count: 0,
    };
    let declarations = owner.run(root).map_err(refuse)?;
    if owner.findings.is_empty() {
        Ok(declarations)
    } else {
        Err(refuse(Cause::Findings(owner.findings)))
    }
}
struct Parser<'a, 'b> {
    input: &'a Document,
    budget: Option<&'b mut Budget>,
    model: bool,
    findings: Vec<Finding>,
    count: usize,
}
#[derive(Clone, Copy)]
enum TextRule {
    Any,
    NonEmpty,
    Identifier,
    Integer,
}
impl<'a> Parser<'a, '_> {
    fn fault_at(
        &mut self,
        node: usize,
        origin: Option<Origin>,
        field: Option<(usize, Option<usize>)>,
        path: Path,
        kind: Kind,
        requirement: Option<&'static str>,
    ) -> Result<(), Cause> {
        if let Some(budget) = &mut self.budget {
            budget.charge(Resource::Findings, 1)?;
            budget.charge(Resource::PathSegments, path.len)?;
            // Both non-owning count and final diagnostic ownership are bounded.
            budget.multiply(Resource::Work, path.len + 1, 2)?;
        } else {
            self.findings.push(Finding {
                node,
                container: field.map(|v| v.0),
                key: field.and_then(|v| v.1),
                origin,
                path,
                kind,
                requirement,
            });
        }
        self.count = self
            .count
            .checked_add(1)
            .ok_or(Cause::Limit(Resource::Findings))?;
        Ok(())
    }
    fn fault(
        &mut self,
        origin: Origin,
        path: Path,
        kind: Kind,
        requirement: Option<&'static str>,
    ) -> Result<(), Cause> {
        self.fault_at(
            origin.node.unwrap_or(origin.container),
            Some(origin),
            Some((origin.container, origin.key)),
            path,
            kind,
            requirement,
        )
    }
    fn get(&self, node: usize, field: F) -> Origin {
        occurrence(self.input, node, field)
    }
    fn mapping(
        &mut self,
        origin: Origin,
        path: Path,
        allowed: &[F],
        column: bool,
    ) -> Result<bool, Cause> {
        let Some(node) = origin.node else {
            self.fault(origin, path, Kind::Missing, None)?;
            return Ok(false);
        };
        let N::Mapping(fields) = &self.input.nodes()[node] else {
            self.fault(origin, path, Kind::ExpectedMapping, None)?;
            return Ok(false);
        };
        for &(key, value) in fields {
            match &self.input.nodes()[key] {
                N::Text(text)
                    if allowed.iter().any(|v| v.name() == text)
                        || (column
                            && matches!(
                                text.as_str(),
                                "derivation" | "verifications" | "metadata"
                            )) => {}
                N::Text(_) => self.fault_at(
                    value,
                    None,
                    Some((node, Some(key))),
                    path,
                    Kind::UnknownField,
                    None,
                )?,
                _ => self.fault_at(
                    key,
                    None,
                    Some((node, Some(key))),
                    path,
                    Kind::NonTextKey,
                    None,
                )?,
            }
        }
        Ok(true)
    }
    fn fields(&self, node: usize, allowed: &[F]) -> Vec<Origin> {
        if !self.model {
            return Vec::new();
        }
        let N::Mapping(fields) = &self.input.nodes()[node] else {
            unreachable!("accepted mapping")
        };
        let mut owned = Vec::with_capacity(fields.len());
        for &(key, value) in fields {
            if let N::Text(text) = &self.input.nodes()[key] {
                if let Some(&(_, field)) = F::VALUES
                    .iter()
                    .find(|&&(name, field)| name == text && allowed.contains(&field))
                {
                    owned.push(Origin {
                        container: node,
                        field,
                        key: Some(key),
                        node: Some(value),
                    });
                }
            }
        }
        owned
    }
    fn text(
        &mut self,
        origin: Origin,
        path: Path,
        rule: TextRule,
        required: bool,
    ) -> Result<Option<Field<String>>, Cause> {
        let Some(node) = origin.node else {
            if required {
                self.fault(origin, path, Kind::Missing, None)?;
            }
            return Ok(None);
        };
        let value = match (&self.input.nodes()[node], rule) {
            (N::Text(text), TextRule::Any | TextRule::NonEmpty | TextRule::Identifier) => text,
            (N::Integer(text), TextRule::Integer) => text,
            _ => {
                self.fault(
                    origin,
                    path,
                    if matches!(rule, TextRule::Integer) {
                        Kind::ExpectedInteger
                    } else {
                        Kind::ExpectedText
                    },
                    None,
                )?;
                return Ok(None);
            }
        };
        let failure = match rule {
            TextRule::Identifier if !identifier(value) => {
                Some((Kind::InvalidIdentifier, Some("REQ-1046")))
            }
            TextRule::NonEmpty if value.is_empty() => Some((Kind::EmptyText, None)),
            _ => None,
        };
        if let Some((kind, requirement)) = failure {
            self.fault(origin, path, kind, requirement)?;
            return Ok(None);
        }
        Ok(Some(Field {
            value: if self.model {
                value.clone()
            } else {
                String::new()
            },
            origin,
        }))
    }
    fn text_field(
        &mut self,
        node: usize,
        field: F,
        path: Path,
        rule: TextRule,
        required: bool,
    ) -> Result<Option<Field<String>>, Cause> {
        self.text(self.get(node, field), path.field(field), rule, required)
    }
    fn choice<T: Copy>(
        &mut self,
        origin: Origin,
        path: Path,
        values: &[(&str, T)],
        default: Option<T>,
        required: bool,
        requirement: Option<&'static str>,
    ) -> Result<Option<Field<T>>, Cause> {
        let Some(node) = origin.node else {
            if let Some(value) = default {
                return Ok(Some(Field { value, origin }));
            }
            if required {
                self.fault(origin, path, Kind::Missing, None)?;
            }
            return Ok(None);
        };
        let N::Text(text) = &self.input.nodes()[node] else {
            self.fault(origin, path, Kind::ExpectedText, None)?;
            return Ok(None);
        };
        if let Some(&(_, value)) = values.iter().find(|v| v.0 == text) {
            Ok(Some(Field { value, origin }))
        } else {
            self.fault(origin, path, Kind::InvalidChoice, requirement)?;
            Ok(None)
        }
    }
    fn boolean(
        &mut self,
        origin: Origin,
        path: Path,
        default: Option<bool>,
        required: bool,
    ) -> Result<Option<Field<bool>>, Cause> {
        let Some(node) = origin.node else {
            if let Some(value) = default {
                return Ok(Some(Field { value, origin }));
            }
            if required {
                self.fault(origin, path, Kind::Missing, None)?;
            }
            return Ok(None);
        };
        if let N::Boolean(value) = self.input.nodes()[node] {
            Ok(Some(Field { value, origin }))
        } else {
            self.fault(origin, path, Kind::ExpectedBoolean, None)?;
            Ok(None)
        }
    }
    fn sequence(&mut self, origin: Origin, path: Path) -> Result<Option<&'a [usize]>, Cause> {
        let Some(node) = origin.node else {
            return Ok(Some(&[]));
        };
        match &self.input.nodes()[node] {
            N::Sequence(items) => Ok(Some(items)),
            _ => {
                self.fault(origin, path, Kind::ExpectedSequence, None)?;
                Ok(None)
            }
        }
    }
    fn dataset(&mut self, origin: Origin, path: Path) -> Result<Option<Dataset>, Cause> {
        if !self.mapping(origin, path, DATASET, false)? {
            return Ok(None);
        }
        let node = origin.node.unwrap();
        let has_no_data = self.boolean(
            self.get(node, F::HasNoData),
            path.field(F::HasNoData),
            Some(false),
            false,
        )?;
        let label = self.text_field(node, F::Label, path, TextRule::NonEmpty, true)?;
        let class = self.choice(
            self.get(node, F::Class),
            path.field(F::Class),
            DatasetClass::VALUES,
            None,
            true,
            Some("REQ-1146"),
        )?;
        let subclass = self.choice(
            self.get(node, F::Subclass),
            path.field(F::Subclass),
            DatasetSubclass::VALUES,
            None,
            false,
            Some("REQ-1147"),
        )?;
        let structure = self.text_field(node, F::Structure, path, TextRule::NonEmpty, true)?;
        let repeating = self.boolean(
            self.get(node, F::Repeating),
            path.field(F::Repeating),
            None,
            true,
        )?;
        let reference_data = self.boolean(
            self.get(node, F::ReferenceData),
            path.field(F::ReferenceData),
            Some(false),
            false,
        )?;
        let domain = self.text_field(node, F::Domain, path, TextRule::Identifier, false)?;
        let comment = self.comment(self.get(node, F::Comment), path.field(F::Comment))?;
        Ok(has_no_data
            .zip(label)
            .zip(class)
            .zip(structure)
            .zip(repeating)
            .zip(reference_data)
            .map(
                |(((((has_no_data, label), class), structure), repeating), reference_data)| {
                    Dataset {
                        origin,
                        fields: self.fields(node, DATASET),
                        has_no_data,
                        label,
                        class,
                        subclass,
                        structure,
                        repeating,
                        reference_data,
                        domain,
                        comment,
                    }
                },
            ))
    }
    fn column(&mut self, node: usize, parent: Origin, path: Path) -> Result<Option<Column>, Cause> {
        let origin = Origin {
            container: parent.node.expect("column list occurrence"),
            key: None,
            node: Some(node),
            field: F::Columns,
        };
        if !self.mapping(origin, path, COLUMN, true)? {
            return Ok(None);
        }
        let name = self.text_field(node, F::Name, path, TextRule::Identifier, true)?;
        let column_type = self.choice(
            self.get(node, F::Type),
            path.field(F::Type),
            ColumnType::VALUES,
            None,
            true,
            None,
        )?;
        let label = self.text_field(node, F::Label, path, TextRule::Any, false)?;
        let submission_origin = self.get(node, F::Submission);
        let submission = if submission_origin.node.is_some() {
            self.submission(submission_origin, path.field(F::Submission))?
        } else {
            None
        };
        Ok(name.zip(column_type).map(|(name, column_type)| Column {
            node,
            fields: self.fields(node, COLUMN),
            name,
            column_type,
            label,
            submission,
        }))
    }
    fn value_rows(&mut self, root: usize) -> Result<Vec<ValueRow>, Cause> {
        let Some(N::Sequence(rows)) = self.get(root, F::Rows).node.map(|n| &self.input.nodes()[n])
        else {
            return Ok(Vec::new());
        };
        let mut owned = Vec::new();
        for (index, &node) in rows.iter().enumerate() {
            let origin = self.get(node, F::Submission);
            let Some(submission) = origin.node else {
                continue;
            };
            if matches!(&self.input.nodes()[submission], N::Null) {
                continue;
            }
            let path = Path::root().field(F::Rows).child(Step::Index(index));
            let N::Mapping(fields) = &self.input.nodes()[submission] else {
                self.fault(
                    origin,
                    path.field(F::Submission),
                    Kind::ExpectedMapping,
                    None,
                )?;
                continue;
            };
            if fields.is_empty() {
                continue;
            }
            let id = self.text_field(node, F::Id, path, TextRule::NonEmpty, true)?;
            let mut overrides = Vec::new();
            for (column_index, &(key, value)) in fields.iter().enumerate() {
                let occurrence = Origin {
                    container: submission,
                    key: Some(key),
                    node: Some(value),
                    field: F::Submission,
                };
                let field_path = path.field(F::Submission).child(Step::Index(column_index));
                let name = self.text(
                    Origin {
                        node: Some(key),
                        ..occurrence
                    },
                    field_path,
                    TextRule::Identifier,
                    true,
                )?;
                let metadata = self.submission(occurrence, field_path)?;
                if self.model {
                    if let Some((name, submission)) = name.zip(metadata) {
                        overrides.push(ValueOverride { name, submission });
                    }
                }
            }
            if self.model {
                if let Some(id) = id {
                    owned.push(ValueRow {
                        node,
                        id,
                        origin,
                        overrides,
                    });
                }
            }
        }
        Ok(owned)
    }
    fn submission(
        &mut self,
        origin: Origin,
        path: Path,
    ) -> Result<Option<ColumnSubmission>, Cause> {
        if !self.mapping(origin, path, SUBMISSION, false)? {
            return Ok(None);
        }
        let node = origin.node.unwrap();
        let core = self.choice(
            self.get(node, F::Core),
            path.field(F::Core),
            Core::VALUES,
            None,
            false,
            Some("REQ-1141"),
        )?;
        let mandatory = self.boolean(
            self.get(node, F::Mandatory),
            path.field(F::Mandatory),
            None,
            false,
        )?;
        let role = self.text_field(node, F::Role, path, TextRule::NonEmpty, false)?;
        let data_type = self.choice(
            self.get(node, F::DataType),
            path.field(F::DataType),
            DataType::VALUES,
            None,
            false,
            Some("REQ-1145"),
        )?;
        let length = self.text_field(node, F::Length, path, TextRule::Integer, false)?;
        let significant_digits =
            self.text_field(node, F::SignificantDigits, path, TextRule::Integer, false)?;
        let display_format =
            self.text_field(node, F::DisplayFormat, path, TextRule::NonEmpty, false)?;
        let codelist = self.text_field(node, F::Codelist, path, TextRule::Identifier, false)?;
        let inventory_vocabulary = self.boolean(
            self.get(node, F::InventoryVocabulary),
            path.field(F::InventoryVocabulary),
            None,
            false,
        )?;
        let origin_metadata = self.origin(self.get(node, F::Origin), path.field(F::Origin))?;
        let method = self.method(self.get(node, F::Method), path.field(F::Method))?;
        let comment = self.comment(self.get(node, F::Comment), path.field(F::Comment))?;
        Ok(Some(ColumnSubmission {
            origin,
            fields: self.fields(node, SUBMISSION),
            core,
            mandatory,
            role,
            data_type,
            length,
            significant_digits,
            display_format,
            codelist,
            inventory_vocabulary,
            origin_metadata,
            method,
            comment,
        }))
    }
    fn origin(&mut self, origin: Origin, path: Path) -> Result<Option<OriginMetadata>, Cause> {
        if origin.node.is_none() {
            return Ok(None);
        }
        if !self.mapping(origin, path, ORIGIN, false)? {
            return Ok(None);
        }
        let node = origin.node.unwrap();
        let kind = self.choice(
            self.get(node, F::Type),
            path.field(F::Type),
            OriginType::VALUES,
            None,
            true,
            Some("REQ-1142"),
        )?;
        let source = self.choice(
            self.get(node, F::Source),
            path.field(F::Source),
            OriginSource::VALUES,
            None,
            false,
            Some("REQ-1143"),
        )?;
        let description = self.text_field(node, F::Description, path, TextRule::NonEmpty, false)?;
        let documents = self.documents(self.get(node, F::Documents), path.field(F::Documents))?;
        Ok(kind.map(|kind| OriginMetadata {
            origin,
            fields: self.fields(node, ORIGIN),
            kind,
            source,
            description,
            documents,
        }))
    }
    fn method(&mut self, origin: Origin, path: Path) -> Result<Option<Method>, Cause> {
        let Some(node) = origin.node else {
            return Ok(None);
        };
        if matches!(self.input.nodes()[node], N::Text(_)) {
            return Ok(self
                .text(origin, path, TextRule::Any, true)?
                .map(Method::Plain));
        }
        if !self.mapping(origin, path, METHOD, false)? {
            return Ok(None);
        }
        let name = self.text_field(node, F::Name, path, TextRule::NonEmpty, false)?;
        let kind = self.choice(
            self.get(node, F::Type),
            path.field(F::Type),
            MethodType::VALUES,
            Some(MethodType::Computation),
            false,
            Some("REQ-1144"),
        )?;
        let description = self.text_field(node, F::Description, path, TextRule::NonEmpty, true)?;
        let expression =
            self.expression(self.get(node, F::Expression), path.field(F::Expression))?;
        let documents = self.documents(self.get(node, F::Documents), path.field(F::Documents))?;
        // Boxes are admitted by the selected-node bound and allocated only in
        // the successful owned pass, never during either finding pass.
        Ok(if self.model {
            kind.zip(description).map(|(kind, description)| {
                Method::Class(Box::new(MethodClass {
                    origin,
                    fields: self.fields(node, METHOD),
                    name,
                    kind,
                    description,
                    expression,
                    documents,
                }))
            })
        } else {
            None
        })
    }
    fn expression(
        &mut self,
        origin: Origin,
        path: Path,
    ) -> Result<Option<FormalExpression>, Cause> {
        if origin.node.is_none() {
            return Ok(None);
        }
        if !self.mapping(origin, path, EXPRESSION, false)? {
            return Ok(None);
        }
        let node = origin.node.unwrap();
        let context = self.text_field(node, F::Context, path, TextRule::NonEmpty, true)?;
        let code = self.text_field(node, F::Code, path, TextRule::NonEmpty, true)?;
        Ok(context.zip(code).map(|(context, code)| FormalExpression {
            origin,
            fields: self.fields(node, EXPRESSION),
            context,
            code,
        }))
    }
    fn comment(&mut self, origin: Origin, path: Path) -> Result<Option<Comment>, Cause> {
        let Some(node) = origin.node else {
            return Ok(None);
        };
        if matches!(self.input.nodes()[node], N::Text(_)) {
            return Ok(self
                .text(origin, path, TextRule::Any, true)?
                .map(Comment::Plain));
        }
        if !self.mapping(origin, path, COMMENT, false)? {
            return Ok(None);
        }
        let text = self.text_field(node, F::Text, path, TextRule::NonEmpty, true)?;
        let documents = self.documents(self.get(node, F::Documents), path.field(F::Documents))?;
        Ok(text.map(|text| {
            Comment::Class(CommentClass {
                origin,
                fields: self.fields(node, COMMENT),
                text,
                documents,
            })
        }))
    }
    fn documents(
        &mut self,
        origin: Origin,
        path: Path,
    ) -> Result<Field<Vec<DocumentReference>>, Cause> {
        // Borrow the immutable sequence with its document lifetime, independently
        // of this parser's mutable borrow while decoding individual references.
        let nodes = self.sequence(origin, path)?.unwrap_or(&[]);
        let mut owned = if self.model {
            Vec::with_capacity(nodes.len())
        } else {
            Vec::new()
        };
        for (index, &node) in nodes.iter().enumerate() {
            let item = Origin {
                container: origin.node.unwrap(),
                key: None,
                node: Some(node),
                field: F::Documents,
            };
            if let Some(value) = self.reference(item, path.child(Step::Index(index)))? {
                if self.model {
                    owned.push(value);
                }
            }
        }
        Ok(Field {
            value: owned,
            origin,
        })
    }
    fn reference(
        &mut self,
        origin: Origin,
        path: Path,
    ) -> Result<Option<DocumentReference>, Cause> {
        let node = origin.node.unwrap();
        if matches!(self.input.nodes()[node], N::Text(_)) {
            return Ok(self
                .text(origin, path, TextRule::Identifier, true)?
                .map(DocumentReference::Plain));
        }
        if !self.mapping(origin, path, REFERENCE, false)? {
            return Ok(None);
        }
        let document = self.text_field(node, F::Document, path, TextRule::Identifier, true)?;
        let pages = self.pages(self.get(node, F::Pages), path.field(F::Pages))?;
        Ok(if self.model {
            document.map(|document| {
                DocumentReference::Class(Box::new(DocumentReferenceClass {
                    origin,
                    fields: self.fields(node, REFERENCE),
                    document,
                    pages,
                }))
            })
        } else {
            None
        })
    }
    fn pages(&mut self, origin: Origin, path: Path) -> Result<Option<Pages>, Cause> {
        if origin.node.is_none() {
            return Ok(None);
        }
        if !self.mapping(origin, path, PAGES, false)? {
            return Ok(None);
        }
        let node = origin.node.unwrap();
        let kind = self.choice(
            self.get(node, F::Type),
            path.field(F::Type),
            PageType::VALUES,
            None,
            true,
            None,
        )?;
        let refs = self.text_field(node, F::Refs, path, TextRule::NonEmpty, true)?;
        let title = self.text_field(node, F::Title, path, TextRule::NonEmpty, false)?;
        Ok(kind.zip(refs).map(|(kind, refs)| Pages {
            origin,
            fields: self.fields(node, PAGES),
            kind,
            refs,
            title,
        }))
    }
    fn run(&mut self, root: usize) -> Result<Declarations, Cause> {
        let N::Mapping(fields) = &self.input.nodes()[root] else {
            unreachable!("admitted root")
        };
        let input = self.input;
        let mut dataset = None;
        let mut columns = None;
        for &(key, node) in fields {
            let N::Text(text) = &input.nodes()[key] else {
                self.fault_at(
                    key,
                    None,
                    Some((root, Some(key))),
                    Path::root(),
                    Kind::NonTextKey,
                    None,
                )?;
                continue;
            };
            if text == "submission" {
                dataset = self.dataset(
                    self.get(root, F::Submission),
                    Path::root().field(F::Submission),
                )?;
            } else if text == "columns" {
                let origin = self.get(root, F::Columns);
                self.sequence(origin, Path::root().field(F::Columns))?;
                let N::Sequence(items) = &input.nodes()[node] else {
                    continue;
                };
                let mut owned = if self.model {
                    Vec::with_capacity(items.len())
                } else {
                    Vec::new()
                };
                for (index, &item) in items.iter().enumerate() {
                    if let Some(column) = self.column(
                        item,
                        origin,
                        Path::root().field(F::Columns).child(Step::Index(index)),
                    )? {
                        if self.model {
                            owned.push(column);
                        }
                    }
                }
                columns = Some(Field {
                    value: owned,
                    origin,
                });
            }
        }
        Ok(Declarations {
            root,
            fields: self.fields(root, &[F::Submission, F::Columns, F::Rows]),
            dataset,
            columns,
            value_rows: self.value_rows(root)?,
        })
    }
}
fn identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b) if b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
