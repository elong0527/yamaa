//! One owned semantic decode of the held input, retaining exact occurrence facts.
use super::*;
use crate::cdisc_vocabulary::{
    Core, DataType, DatasetClass, DatasetSubclass, MethodType, OriginSource, OriginType, PageType,
};
use crate::diagnostic::ScalarSubmissionRule;

use FieldName as F;

const DATASET: &[F] = DATASET_METADATA_FIELDS;
const COLUMN: &[F] = &[F::Name, F::Type, F::Label, F::Submission];
/// A column's governed fields, where a 1.0 specification writes its origin
/// as `origins` (P-ORIGIN-1).
const SUBMISSION: &[F] = &[
    F::KeySequence,
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
    F::Origins,
    F::Method,
    F::Comment,
    F::ValueLevel,
];
/// An entry's fields, with a 1.0 entry's `origins` (P-ORIGIN-1).
const ENTRY: &[F] = &[
    F::KeySequence,
    F::Id,
    F::Where,
    F::Label,
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
    F::Origins,
    F::Method,
    F::Comment,
];
/// The metadata fields an entry shares with its column.
const ENTRY_METADATA: &[F] = &[
    F::KeySequence,
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
            steps: [None; 11],
            len: 0,
        }
    }
    fn child(mut self, step: Step) -> Self {
        // A value-level entry's deepest pages field reaches ten segments, and
        // eleven through a 1.0 entry's `origins[0]` (P-ORIGIN-1).
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
pub fn decode(input: &Document, root: usize) -> Result<Declarations, Error<'_>> {
    let refuse = |cause| Error { input, cause };
    let Some(N::Mapping(_)) = input.nodes().get(root) else {
        return Err(refuse(Cause::Shape { node: root }));
    };
    if let Some(node) = input.field(root, "columns") {
        if let N::Sequence(columns) = &input.nodes()[node] {
            crate::limits::Limits::columns(columns.len())
                .map_err(|error| refuse(Cause::Resource(error)))?;
        }
    }
    let mut owner = Parser {
        input,
        findings: Vec::new(),
    };
    let declarations = owner.run(root).map_err(refuse)?;
    if owner.findings.is_empty() {
        Ok(declarations)
    } else {
        Err(refuse(Cause::Findings(owner.findings)))
    }
}
struct Parser<'a> {
    input: &'a Document,
    findings: Vec<Finding>,
}
#[derive(Clone, Copy)]
enum TextRule {
    Any,
    NonEmpty,
    Identifier,
    Integer,
}
impl<'a> Parser<'a> {
    fn fault_at(
        &mut self,
        node: usize,
        origin: Option<Origin>,
        field: Option<(usize, Option<usize>)>,
        path: Path,
        kind: Kind,
        scalar_rule: Option<ScalarSubmissionRule>,
    ) -> Result<(), Cause> {
        self.findings.push(Finding {
            node,
            container: field.map(|v| v.0),
            key: field.and_then(|v| v.1),
            origin,
            path,
            kind,
            scalar_rule,
        });

        Ok(())
    }
    fn fault(
        &mut self,
        origin: Origin,
        path: Path,
        kind: Kind,
        scalar_rule: Option<ScalarSubmissionRule>,
    ) -> Result<(), Cause> {
        self.fault_at(
            origin.node.unwrap_or(origin.container),
            Some(origin),
            Some((origin.container, origin.key)),
            path,
            kind,
            scalar_rule,
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
            TextRule::Identifier if !identifier(value) => Some((
                Kind::InvalidIdentifier,
                Some(ScalarSubmissionRule::Identifier),
            )),
            TextRule::NonEmpty if value.is_empty() => Some((Kind::EmptyText, None)),
            _ => None,
        };
        if let Some((kind, scalar_rule)) = failure {
            self.fault(origin, path, kind, scalar_rule)?;
            return Ok(None);
        }
        Ok(Some(Field {
            value: { value.clone() },
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
        scalar_rule: Option<ScalarSubmissionRule>,
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
            self.fault(origin, path, Kind::InvalidChoice, scalar_rule)?;
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
            Some(ScalarSubmissionRule::DatasetClass),
        )?;
        let subclass = self.choice(
            self.get(node, F::Subclass),
            path.field(F::Subclass),
            DatasetSubclass::VALUES,
            None,
            false,
            Some(ScalarSubmissionRule::DatasetSubclass),
        )?;
        let Some(mode) = self.choice(
            self.get(node, F::Mode),
            path.field(F::Mode),
            SubmissionMode::VALUES,
            Some(SubmissionMode::Validated),
            false,
            None,
        )?
        else {
            return Ok(None);
        };
        let artifact = self.text_field(node, F::Artifact, path, TextRule::NonEmpty, false)?;
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
        // A guide name such as `SDTMIG-MD` is no identifier; the section's
        // `standards` resolves it (DEFINE-8).
        let standard = self.text_field(node, F::Standard, path, TextRule::NonEmpty, false)?;
        let non_standard = self.boolean(
            self.get(node, F::NonStandard),
            path.field(F::NonStandard),
            Some(false),
            false,
        )?;
        let comment = self.comment(self.get(node, F::Comment), path.field(F::Comment))?;
        let Some(non_standard) = non_standard else {
            return Ok(None);
        };
        Ok(has_no_data
            .zip(label)
            .zip(class)
            .zip(structure)
            .zip(repeating)
            .zip(reference_data)
            .map(
                |(((((has_no_data, label), class), structure), repeating), reference_data)| {
                    Dataset {
                        mode: mode.clone(),
                        artifact: artifact.clone(),
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
                        standard,
                        non_standard,
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
    fn submission(
        &mut self,
        origin: Origin,
        path: Path,
    ) -> Result<Option<ColumnSubmission>, Cause> {
        if !self.mapping(origin, path, SUBMISSION, false)? {
            return Ok(None);
        }
        let node = origin.node.unwrap();
        let mut metadata = self.metadata(origin, node, path, SUBMISSION)?;
        metadata.value_level =
            self.entries(self.get(node, F::ValueLevel), path.field(F::ValueLevel))?;
        Ok(Some(metadata))
    }
    fn entries(&mut self, origin: Origin, path: Path) -> Result<Vec<ValueEntry>, Cause> {
        let nodes = self.sequence(origin, path)?.unwrap_or(&[]);
        let mut owned = Vec::with_capacity(nodes.len());
        for (index, &node) in nodes.iter().enumerate() {
            let item = Origin {
                container: origin.node.unwrap(),
                key: None,
                node: Some(node),
                field: F::ValueLevel,
            };
            if let Some(entry) = self.entry(item, path.child(Step::Index(index)))? {
                owned.push(entry);
            }
        }
        Ok(owned)
    }
    fn entry(&mut self, origin: Origin, path: Path) -> Result<Option<ValueEntry>, Cause> {
        if !self.mapping(origin, path, ENTRY, false)? {
            return Ok(None);
        }
        let node = origin.node.unwrap();
        let id = self.text_field(node, F::Id, path, TextRule::Identifier, true)?;
        let clause = self.clause(self.get(node, F::Where), path.field(F::Where))?;
        let label = self.text_field(node, F::Label, path, TextRule::NonEmpty, false)?;
        let metadata = self.metadata(origin, node, path, ENTRY_METADATA)?;
        Ok(id.zip(clause).map(|(id, clause)| ValueEntry {
            origin,
            fields: self.fields(node, ENTRY),
            id,
            clause,
            label,
            metadata,
        }))
    }
    /// A where clause: each declared column and the nonempty values it admits.
    fn clause(&mut self, origin: Origin, path: Path) -> Result<Option<Vec<WhereTerm>>, Cause> {
        let Some(node) = origin.node else {
            self.fault(origin, path, Kind::Missing, None)?;
            return Ok(None);
        };
        let N::Mapping(fields) = &self.input.nodes()[node] else {
            self.fault(origin, path, Kind::ExpectedMapping, None)?;
            return Ok(None);
        };
        let mut terms = Vec::with_capacity(fields.len());
        let mut complete = true;
        for (index, &(key, value)) in fields.iter().enumerate() {
            let occurrence = Origin {
                container: node,
                key: Some(key),
                node: Some(value),
                field: F::Where,
            };
            let term = path.child(Step::Index(index));
            let column = self.text(
                Origin {
                    node: Some(key),
                    ..occurrence
                },
                term,
                TextRule::Identifier,
                true,
            )?;
            // A resolved 1.0 specification holds its where condition in the
            // long form, `{values: [...]}`, with an `NE` or `NOTIN`
            // comparator when one is written (P-VLM-1).
            let (comparator, list) = match &self.input.nodes()[value] {
                N::Mapping(_) => (
                    match self
                        .input
                        .field(value, "comparator")
                        .map(|id| &self.input.nodes()[id])
                    {
                        Some(N::Text(written)) => crate::ast::Comparator::written_as(written),
                        Some(_) => crate::ast::Comparator::NotIn,
                        None => crate::ast::Comparator::In,
                    },
                    self.input.field(value, "values").unwrap_or(value),
                ),
                _ => (crate::ast::Comparator::In, value),
            };
            let occurrence = Origin {
                container: if list == value { node } else { value },
                node: Some(list),
                ..occurrence
            };
            let Some(items) = self.sequence(occurrence, term)? else {
                complete = false;
                continue;
            };
            let mut values = Vec::with_capacity(items.len());
            for (position, &item) in items.iter().enumerate() {
                let item = Origin {
                    container: list,
                    key: None,
                    node: Some(item),
                    field: F::Where,
                };
                match self.text(
                    item,
                    term.child(Step::Index(position)),
                    TextRule::NonEmpty,
                    true,
                )? {
                    Some(text) => values.push(text),
                    None => complete = false,
                }
            }
            match column {
                Some(column) => terms.push(WhereTerm {
                    column,
                    comparator,
                    values,
                }),
                None => complete = false,
            }
        }
        Ok(complete.then_some(terms))
    }
    /// The metadata fields a column and each of its entries share.
    fn metadata(
        &mut self,
        origin: Origin,
        node: usize,
        path: Path,
        allowed: &[F],
    ) -> Result<ColumnSubmission, Cause> {
        let core = self.choice(
            self.get(node, F::Core),
            path.field(F::Core),
            Core::VALUES,
            None,
            false,
            Some(ScalarSubmissionRule::Core),
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
            Some(ScalarSubmissionRule::DataType),
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
        let origins = self.origins_of(node, path)?;
        let method = self.method(self.get(node, F::Method), path.field(F::Method))?;
        let comment = self.comment(self.get(node, F::Comment), path.field(F::Comment))?;
        Ok(ColumnSubmission {
            key_sequence: self.text_field(node, F::KeySequence, path, TextRule::Integer, false)?,
            origin,
            fields: self.fields(node, allowed),
            core,
            mandatory,
            role,
            data_type,
            length,
            significant_digits,
            display_format,
            codelist,
            inventory_vocabulary,
            origins,
            method,
            comment,
            value_level: Vec::new(),
        })
    }
    /// The origins `node` declares, in declaration order: a 0.5 `origin`, or
    /// each entry of a 1.0 `origins` (P-ORIGIN-1).
    fn origins_of(&mut self, node: usize, path: Path) -> Result<Vec<OriginMetadata>, Cause> {
        let origins = self.get(node, F::Origins);
        let Some(list) = origins.node else {
            return Ok(self
                .origin(self.get(node, F::Origin), path.field(F::Origin))?
                .into_iter()
                .collect());
        };
        let path = path.field(F::Origins);
        let N::Sequence(items) = &self.input.nodes()[list] else {
            self.fault(origins, path, Kind::ExpectedSequence, None)?;
            return Ok(Vec::new());
        };
        let mut declared = Vec::with_capacity(items.len());
        for (index, &item) in items.iter().enumerate() {
            let entry = Origin {
                container: list,
                key: None,
                node: Some(item),
                field: F::Origins,
            };
            declared.extend(self.origin(entry, path.child(Step::Index(index)))?);
        }
        Ok(declared)
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
            Some(ScalarSubmissionRule::OriginType),
        )?;
        let source = self.choice(
            self.get(node, F::Source),
            path.field(F::Source),
            OriginSource::VALUES,
            None,
            false,
            Some(ScalarSubmissionRule::OriginSource),
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
            Some(ScalarSubmissionRule::MethodType),
        )?;
        let description = self.text_field(node, F::Description, path, TextRule::NonEmpty, true)?;
        let expression =
            self.expression(self.get(node, F::Expression), path.field(F::Expression))?;
        let documents = self.documents(self.get(node, F::Documents), path.field(F::Documents))?;
        // Boxes are admitted by the selected-node bound and allocated only in
        // the successful owned pass, never during either finding pass.
        Ok({
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
        let mut owned = { Vec::with_capacity(nodes.len()) };
        for (index, &node) in nodes.iter().enumerate() {
            let item = Origin {
                container: origin.node.unwrap(),
                key: None,
                node: Some(node),
                field: F::Documents,
            };
            if let Some(value) = self.reference(item, path.child(Step::Index(index)))? {
                {
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
        Ok({
            document.map(|document| {
                DocumentReference::Class(Box::new(DocumentReferenceClass {
                    origin,
                    fields: self.fields(node, REFERENCE),
                    document,
                    pages,
                }))
            })
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
                let mut owned = { Vec::with_capacity(items.len()) };
                for (index, &item) in items.iter().enumerate() {
                    if let Some(column) = self.column(
                        item,
                        origin,
                        Path::root().field(F::Columns).child(Step::Index(index)),
                    )? {
                        {
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
            fields: self.fields(root, &[F::Submission, F::Columns]),
            dataset,
            columns,
        })
    }
}
fn identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b) if b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_all_10001_non_text_root_key_occurrences() {
        let mut nodes = Vec::new();
        let mut fields = Vec::new();
        for index in 0..10_001 {
            let key = nodes.len();
            nodes.push(N::Integer(index.to_string()));
            let value = nodes.len();
            nodes.push(N::Null);
            fields.push((key, value));
        }
        let root = nodes.len();
        nodes.push(N::Mapping(fields));
        let document = Document::new(nodes, root).unwrap();
        let error = crate::specification_submission_document::decode(&document, root).unwrap_err();
        assert!(std::ptr::eq(error.input, &document));
        let Cause::Findings(findings) = error.cause else {
            panic!("every malformed key must retain its original occurrence");
        };
        assert_eq!(findings.len(), 10_001);
        for (index, finding) in findings.iter().enumerate() {
            assert_eq!(finding.kind, Kind::NonTextKey);
            assert_eq!(finding.node, 2 * index);
            assert_eq!(finding.container, Some(root));
            assert_eq!(finding.key, Some(2 * index));
            assert_eq!(finding.origin, None);
            assert_eq!(finding.scalar_rule, None);
            assert_eq!(finding.path.steps().next(), None);
            assert_eq!(
                document.nodes()[finding.node],
                N::Integer(index.to_string())
            );
        }
    }
}
