//! The copy entry (SPEC-9; #3030 D19, D70).
//!
//! A `columns` entry `copy: {dataset: D, columns: [...], submission: {...}}`
//! declares one ordinary column per listed name. Before the layers of a 1.x
//! chain compose, each layer's copy entries are replaced by the columns they
//! declare, so composition keys them by name, a later layer overrides or
//! clears a copied column as it would one written out, and the resolved
//! specification holds them as ordinary columns (P8, LAYER-5, LAYER-9).
//!
//! A copied column `X` reads `D.X` unchanged and takes the type, label and
//! content metadata that the producer input `D` links through `schema`
//! declares for `X`: its output contract (INPUT-10), which the port reads.
//! The input, its link and its base are the ones the resolved specification
//! keeps, whichever layer writes them. A column that `output.columns` lists
//! carries one `Predecessor` origin naming `<producer domain>.X` when the
//! specification declares its `model`, with the entry's `core`, `mandatory`
//! and `role`; a column it does not list is internal and carries no
//! submission metadata (SUB-1), unless the entry writes `submission`.

use crate::diagnostic::{ConditionCode, Diagnostic};
use crate::inheritance::VersionedChain;
use crate::inheritance_preparation::{PathPort, VersionedError};
use crate::json::Value;
use crate::producer_contract::{Contract, Field};
use crate::schema::{
    Document, DocumentNode as N, NormalizationError, NormalizationResource, NormalizedDocument,
    SchemaContext as C, SchemaDiagnostic, SchemaOrigin, SchemaSource, SchemaStructure, SpecPath,
};
use crate::value::ColumnType;
use std::collections::{BTreeMap, BTreeSet};

fn text(document: &Document, node: usize) -> Option<&str> {
    match &document.nodes()[node] {
        N::Text(value) => Some(value),
        _ => None,
    }
}

fn items(document: &Document, node: Option<usize>) -> &[usize] {
    match node.map(|node| &document.nodes()[node]) {
        Some(N::Sequence(items)) => items,
        _ => &[],
    }
}

/// The names a copy entry, the mapping `copy` holds, lists, each with the
/// node that lists it.
pub(crate) fn listed(document: &Document, copy: usize) -> Vec<(&str, usize)> {
    items(document, document.field(copy, "columns"))
        .iter()
        .filter_map(|&item| Some((text(document, item)?, item)))
        .collect()
}

/// One copy entry of a layer.
struct Entry<'a> {
    /// Its position in the layer's `columns`.
    index: usize,
    dataset: &'a str,
    /// Each listed name and the node that lists it.
    names: Vec<(&'a str, usize)>,
    /// The entry's own `submission`, holding its core, mandatory and role.
    submission: Option<usize>,
}

impl Entry<'_> {
    fn dataset_path(&self) -> String {
        format!("columns[{}].copy.dataset", self.index)
    }
    fn name_path(&self, listed: usize) -> String {
        format!("columns[{}].copy.columns[{listed}]", self.index)
    }
}

fn entries(document: &Document) -> Vec<Entry<'_>> {
    items(document, document.field(document.root(), "columns"))
        .iter()
        .enumerate()
        .filter_map(|(index, &column)| {
            let copy = document.field(column, "copy")?;
            Some(Entry {
                index,
                dataset: text(document, document.field(copy, "dataset")?)?,
                names: listed(document, copy),
                submission: document.field(copy, "submission"),
            })
        })
        .collect()
}

/// A declared input as the resolved specification keeps it.
#[derive(Clone, Copy, Default)]
struct Input<'a> {
    /// The layer whose `schema` link it keeps, and the link as written.
    link: Option<(usize, &'a str)>,
    /// Whether its paths resolve from the entry (`relative_to: entry`).
    from_entry: bool,
}

/// What the resolved specification declares that its copy entries read,
/// each written by the latest layer that writes it, after that layer's
/// `clear` (LAYER-5, LAYER-9).
#[derive(Default)]
struct Resolved<'a> {
    inputs: BTreeMap<&'a str, Input<'a>>,
    model: bool,
    published: BTreeSet<&'a str>,
}

impl<'a> Resolved<'a> {
    fn of(layers: &[&'a Document]) -> Self {
        let mut resolved = Self::default();
        for (layer, &document) in layers.iter().enumerate() {
            let root = document.root();
            for &clear in items(document, document.field(root, "clear")) {
                let Some(path) = text(document, clear).and_then(SpecPath::parse) else {
                    continue;
                };
                match path.keys() {
                    [field] if field == "model" => resolved.model = false,
                    [input, name, field] if input == "input" => {
                        if let Some(declared) = resolved.inputs.get_mut(name.as_str()) {
                            match field.as_str() {
                                "schema" => declared.link = None,
                                "relative_to" => declared.from_entry = false,
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            if document.field(root, "model").is_some() {
                resolved.model = true;
            }
            if let Some(N::Mapping(inputs)) = document
                .field(root, "input")
                .map(|node| &document.nodes()[node])
            {
                for &(name, declaration) in inputs {
                    let Some(name) = text(document, name) else {
                        continue;
                    };
                    let input = resolved.inputs.entry(name).or_default();
                    if let Some(schema) = document
                        .field(declaration, "schema")
                        .and_then(|node| text(document, node))
                    {
                        input.link = Some((layer, schema));
                    }
                    if let Some(base) = document
                        .field(declaration, "relative_to")
                        .and_then(|node| text(document, node))
                    {
                        input.from_entry = base == "entry";
                    }
                }
            }
            if let Some(output) = document.field(root, "output") {
                resolved.published = items(document, document.field(output, "columns"))
                    .iter()
                    .filter_map(|&item| text(document, item))
                    .collect();
            }
        }
        resolved
    }
}

/// A `schema` link a copy entry reads: the input and the link as the layer
/// numbered `layer` writes it, resolved from the entry when `from_entry`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link<'a> {
    pub dataset: &'a str,
    pub layer: usize,
    pub written: &'a str,
    pub from_entry: bool,
}

/// The copy entries of a 1.x chain's layers, each layer normalized, in
/// contribution order, with what the resolved specification declares that
/// they read.
pub struct Copies<'a> {
    layers: Vec<Vec<Entry<'a>>>,
    resolved: Resolved<'a>,
}

fn finding(path: String, code: ConditionCode, context: Vec<(&'static str, C)>) -> SchemaDiagnostic {
    SchemaDiagnostic {
        path,
        code,
        context,
    }
}

impl<'a> Copies<'a> {
    /// The copy entries of `layers`, or `None` when no layer writes one.
    pub fn of(layers: &[&'a Document]) -> Option<Self> {
        let entries = layers
            .iter()
            .map(|&layer| entries(layer))
            .collect::<Vec<_>>();
        entries.iter().any(|layer| !layer.is_empty()).then(|| Self {
            layers: entries,
            resolved: Resolved::of(layers),
        })
    }

    /// The first layer, in contribution order, with a copy entry whose
    /// `dataset` is not an input the resolved specification declares
    /// (`unknown_field`) or is one without a `schema` link
    /// (`prohibited_construct`), and its findings in written order.
    pub fn dataset_findings(&self) -> Option<(usize, Vec<SchemaDiagnostic>)> {
        self.first(|entry| match self.resolved.inputs.get(entry.dataset) {
            None => vec![finding(
                entry.dataset_path(),
                ConditionCode::CopyUnknownDataset,
                vec![("identifier", C::Text(entry.dataset.into()))],
            )],
            Some(Input { link: None, .. }) => vec![finding(
                entry.dataset_path(),
                ConditionCode::CopyUnlinkedInput,
                vec![("dataset", C::Text(entry.dataset.into()))],
            )],
            Some(_) => Vec::new(),
        })
    }

    /// The links whose producers the copy entries read, each once, in the
    /// order the entries first name them. Only after
    /// [`Copies::dataset_findings`] finds nothing does every entry's dataset
    /// have one.
    pub fn links(&self) -> Vec<Link<'a>> {
        let mut links: Vec<Link<'a>> = Vec::new();
        for entry in self.layers.iter().flatten() {
            let Some(&Input {
                link: Some((layer, written)),
                from_entry,
            }) = self.resolved.inputs.get(entry.dataset)
            else {
                continue;
            };
            if links.iter().all(|link| link.dataset != entry.dataset) {
                links.push(Link {
                    dataset: entry.dataset,
                    layer,
                    written,
                    from_entry,
                });
            }
        }
        links
    }

    /// The first layer with a listed name that is not an output column of
    /// its dataset's producer, and its `unknown_field` findings in written
    /// order. `contracts` holds each linked dataset's output contract.
    pub fn column_findings(
        &self,
        contracts: &BTreeMap<&str, Contract>,
    ) -> Option<(usize, Vec<SchemaDiagnostic>)> {
        self.first(|entry| {
            let Some(contract) = contracts.get(entry.dataset) else {
                return Vec::new();
            };
            entry
                .names
                .iter()
                .enumerate()
                .filter(|(_, (name, _))| contract.fields().iter().all(|f| f.name != *name))
                .map(|(listed, (name, _))| {
                    finding(
                        entry.name_path(listed),
                        ConditionCode::CopyUnknownColumn,
                        vec![("identifier", C::Text(format!("{}.{name}", entry.dataset)))],
                    )
                })
                .collect()
        })
    }

    fn first(
        &self,
        check: impl Fn(&Entry<'a>) -> Vec<SchemaDiagnostic>,
    ) -> Option<(usize, Vec<SchemaDiagnostic>)> {
        self.layers.iter().enumerate().find_map(|(layer, entries)| {
            let findings = entries.iter().flat_map(&check).collect::<Vec<_>>();
            (!findings.is_empty()).then_some((layer, findings))
        })
    }

    /// The raw columns the copy entries of the layer numbered `layer`, in
    /// `document`, declare, in written order: a document whose root holds
    /// them as `columns`, and for each the node in `document` that lists its
    /// name. Normalized like any layer, they replace the entries.
    fn written(
        &self,
        layer: usize,
        document: &Document,
        contracts: &BTreeMap<&str, Contract>,
    ) -> Result<(Document, Vec<usize>), NormalizationError> {
        let mut raw = Raw::default();
        let mut columns = Vec::new();
        let mut listing = Vec::new();
        for entry in &self.layers[layer] {
            let contract = &contracts[entry.dataset];
            for &(name, node) in &entry.names {
                let field = contract
                    .fields()
                    .iter()
                    .find(|field| field.name == name)
                    .expect("every listed name is an output column");
                let submission = entry.submission.is_some()
                    || (self.resolved.model && self.resolved.published.contains(name));
                columns.push(raw.column(
                    entry,
                    field,
                    contract.domain(),
                    submission.then_some(document),
                ));
                listing.push(node);
            }
        }
        let columns = raw.push(N::Sequence(columns));
        let root = raw.mapping(vec![("columns", columns)]);
        let document = Document::new(raw.nodes, root).map_err(NormalizationError::Document)?;
        Ok((document, listing))
    }
}

/// A raw document under construction, each node after the nodes it holds.
#[derive(Default)]
struct Raw {
    nodes: Vec<N>,
}

impl Raw {
    fn push(&mut self, node: N) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }
    fn text(&mut self, value: &str) -> usize {
        self.push(N::Text(value.into()))
    }
    fn mapping(&mut self, entries: Vec<(&str, usize)>) -> usize {
        let entries = entries
            .into_iter()
            .map(|(key, value)| (self.text(key), value))
            .collect();
        self.push(N::Mapping(entries))
    }
    /// The column `field` of the producer of `entry.dataset`, whose domain is
    /// `domain`, with submission metadata when `layer`, the document holding
    /// `entry`, is given.
    fn column(
        &mut self,
        entry: &Entry<'_>,
        field: &Field,
        domain: &str,
        layer: Option<&Document>,
    ) -> usize {
        let mut column = vec![
            ("name", self.text(&field.name)),
            ("type", self.text(field.kind.name())),
            ("label", self.text(&field.label)),
            (
                "derivation",
                self.text(&format!("{}.{}", entry.dataset, field.name)),
            ),
        ];
        if let Some(layer) = layer {
            let mut submission = Vec::new();
            if let Some(declared) = entry.submission {
                for key in ["core", "mandatory", "role"] {
                    let value = layer
                        .field(declared, key)
                        .map(|value| &layer.nodes()[value]);
                    if let Some(value @ (N::Text(_) | N::Boolean(_))) = value {
                        submission.push((key, self.push(value.clone())));
                    }
                }
            }
            let content = &field.content;
            // A date or datetime is submitted as the consumer's model
            // requires, so it takes no data type, length or display format
            // from its producer (SUB-6, SUB-7, SUB-25).
            let temporal = matches!(field.kind, ColumnType::Date | ColumnType::DateTime);
            for (key, value, integer) in [
                ("data_type", &content.data_type, false),
                ("length", &content.length, true),
                ("significant_digits", &content.significant_digits, true),
                ("display_format", &content.display_format, false),
                ("codelist", &content.codelist, false),
            ] {
                let Some(value) = value.as_deref() else {
                    continue;
                };
                if temporal && key != "codelist" {
                    continue;
                }
                let node = if integer {
                    self.push(N::Integer(value.into()))
                } else {
                    self.text(value)
                };
                submission.push((key, node));
            }
            let kind = self.text("Predecessor");
            let description = self.text(&format!("{domain}.{}", field.name));
            let origin = self.mapping(vec![("type", kind), ("description", description)]);
            let origins = self.push(N::Sequence(vec![origin]));
            submission.push(("origins", origins));
            column.push(("submission", self.mapping(submission)));
        }
        self.mapping(column)
    }
}

/// The refusal of a copy whose producer, through its own copies, leads back
/// to a specification being resolved: the link of input `dataset` at
/// `input.<dataset>.schema`, `cycle` naming the specifications from the
/// first one entered back to itself (SPEC-9).
pub fn producer_cycle(dataset: &str, cycle: &[&str]) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::CopyProducerCycle,
        spec_paths: vec![format!("input.{dataset}.schema")],
        context: [
            ("dataset".into(), Value::String(dataset.into())),
            (
                "cycle".into(),
                Value::Array(cycle.iter().map(|&id| Value::String(id.into())).collect()),
            ),
        ]
        .into_iter()
        .collect(),
        source_span: None,
        operand_route: None,
    }
}

/// Replace every copy entry of `layers`, the normalized layers of `chain`,
/// by the columns it declares, reading each linked producer's output
/// contract through `port` (SPEC-9). A layer without a copy entry is kept as
/// it is.
pub(crate) fn expand<P: PathPort>(
    schema: &SchemaStructure,
    chain: &VersionedChain,
    layers: &mut [NormalizedDocument],
    port: &mut P,
) -> Result<(), VersionedError<P::Error>> {
    let documents = layers
        .iter()
        .map(|layer| &layer.document)
        .collect::<Vec<_>>();
    let Some(copies) = Copies::of(&documents) else {
        return Ok(());
    };
    if let Some((layer, findings)) = copies.dataset_findings() {
        return Err(VersionedError::Copy { layer, findings });
    }
    let entry = &chain.layers.last().expect("a chain holds its entry").source;
    let mut contracts = BTreeMap::new();
    for link in copies.links() {
        let declaring = if link.from_entry {
            entry
        } else {
            &chain.layers[link.layer].source
        };
        let Some(contract) = port.producer(declaring, link.written, link.dataset) else {
            return Err(VersionedError::Producer {
                dataset: link.dataset.into(),
            });
        };
        contracts.insert(link.dataset, contract);
    }
    if let Some((layer, findings)) = copies.column_findings(&contracts) {
        return Err(VersionedError::Copy { layer, findings });
    }
    let mut expanded = Vec::with_capacity(layers.len());
    for (index, layer) in layers.iter().enumerate() {
        if copies.layers[index].is_empty() {
            expanded.push(None);
            continue;
        }
        let (written, listing) =
            copies
                .written(index, &layer.document, &contracts)
                .map_err(|error| VersionedError::Layer {
                    layer: index,
                    error,
                })?;
        // Every value the columns take was admitted in the producer, under
        // the vocabularies the consumer's version shares.
        let normalized = schema
            .at_version(chain.layers[index].version)
            .normalize_versioned_layer(&written)
            .map_err(|error| VersionedError::Layer {
                layer: index,
                error,
            })?;
        let replaced = Splice {
            layer,
            copies: copies.layers[index]
                .iter()
                .map(|entry| (entry.index, entry.names.len()))
                .collect(),
            written: &normalized,
            listing: &listing,
        }
        .run()
        .map_err(|error| VersionedError::Layer {
            layer: index,
            error,
        })?;
        expanded.push(Some(replaced));
    }
    for (layer, replaced) in layers.iter_mut().zip(expanded) {
        if let Some(replaced) = replaced {
            *layer = replaced;
        }
    }
    Ok(())
}

/// A normalized layer with each copy entry replaced by the normalized
/// columns it declares, each node of which keeps the origin of the name that
/// lists its column.
struct Splice<'a> {
    layer: &'a NormalizedDocument,
    /// Each copy entry's position in `columns` and how many columns it lists.
    copies: Vec<(usize, usize)>,
    /// The normalized written columns, in the order the entries list them.
    written: &'a NormalizedDocument,
    /// For each written column, the layer node that lists its name.
    listing: &'a [usize],
}

#[derive(Default)]
struct Arena {
    nodes: Vec<N>,
    origins: Vec<SchemaOrigin>,
}

impl Arena {
    /// Copy `node` of `from` with what it holds; with `at`, every copied node
    /// written in its layer takes that layer node instead.
    fn copy(
        &mut self,
        from: &NormalizedDocument,
        node: usize,
        at: Option<usize>,
        depth: usize,
    ) -> Result<usize, NormalizationError> {
        let limit = crate::limits::Limits::SEMANTIC_DEPTH;
        if depth > limit {
            return Err(NormalizationError::Limit {
                resource: NormalizationResource::Depth,
                limit,
            });
        }
        let value = match &from.document.nodes()[node] {
            N::Sequence(items) => N::Sequence(
                items
                    .iter()
                    .map(|&item| self.copy(from, item, at, depth + 1))
                    .collect::<Result<_, _>>()?,
            ),
            N::Mapping(items) => N::Mapping(
                items
                    .iter()
                    .map(|&(key, value)| {
                        Ok((
                            self.copy(from, key, at, depth + 1)?,
                            self.copy(from, value, at, depth + 1)?,
                        ))
                    })
                    .collect::<Result<_, NormalizationError>>()?,
            ),
            value => value.clone(),
        };
        let mut origin = from.origins[node];
        if let (Some(at), SchemaSource::Input) = (at, origin.source) {
            origin.node = at;
        }
        Ok(self.push(value, origin))
    }
    fn push(&mut self, value: N, origin: SchemaOrigin) -> usize {
        self.nodes.push(value);
        self.origins.push(origin);
        self.nodes.len() - 1
    }
}

impl Splice<'_> {
    fn run(self) -> Result<NormalizedDocument, NormalizationError> {
        let document = &self.layer.document;
        let root = document.root();
        let N::Mapping(fields) = &document.nodes()[root] else {
            return Err(crate::schema::ValidationError::InvalidDescriptor.into());
        };
        let written = &self.written.document;
        let declared = items(written, written.field(written.root(), "columns"));
        let mut arena = Arena::default();
        let mut output = Vec::with_capacity(fields.len());
        for &(key, value) in fields {
            let copied_key = arena.copy(self.layer, key, None, 1)?;
            let copied = if text(document, key) == Some("columns") {
                let N::Sequence(columns) = &document.nodes()[value] else {
                    return Err(crate::schema::ValidationError::InvalidDescriptor.into());
                };
                let mut next = 0;
                let mut members = Vec::new();
                for (index, &column) in columns.iter().enumerate() {
                    match self.copies.iter().find(|(at, _)| *at == index) {
                        Some(&(_, count)) => {
                            for (offset, &declared) in
                                declared[next..next + count].iter().enumerate()
                            {
                                let at = self.layer.origins[self.listing[next + offset]].node;
                                members.push(arena.copy(self.written, declared, Some(at), 2)?);
                            }
                            next += count;
                        }
                        None => members.push(arena.copy(self.layer, column, None, 2)?),
                    }
                }
                arena.push(N::Sequence(members), self.layer.origins[value])
            } else {
                arena.copy(self.layer, value, None, 1)?
            };
            output.push((copied_key, copied));
        }
        let root = arena.push(N::Mapping(output), self.layer.origins[root]);
        let document = Document::new(arena.nodes, root).map_err(NormalizationError::Document)?;
        Ok(NormalizedDocument {
            document,
            origins: arena.origins,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_document::Tree as T;

    #[test]
    fn the_resolved_link_is_the_latest_layer_s_after_its_clears() {
        let parent = T::map(vec![
            (
                "input",
                T::map(vec![(
                    "ADSL",
                    T::map(vec![
                        ("path", T::text("adsl.csv")),
                        ("schema", T::text("adsl.yaml")),
                        ("relative_to", T::text("entry")),
                    ]),
                )]),
            ),
            ("model", T::text("ADaM")),
            (
                "columns",
                T::list(vec![T::map(vec![(
                    "copy",
                    T::map(vec![
                        ("dataset", T::text("ADSL")),
                        ("columns", T::list(vec![T::text("AGE")])),
                    ]),
                )])]),
            ),
        ])
        .document();
        let child = T::map(vec![
            (
                "clear",
                T::list(vec![T::text("input.ADSL.relative_to"), T::text("model")]),
            ),
            (
                "output",
                T::map(vec![("columns", T::list(vec![T::text("AGE")]))]),
            ),
        ])
        .document();
        let copies = Copies::of(&[&parent, &child]).unwrap();
        assert_eq!(copies.dataset_findings(), None);
        assert_eq!(
            copies.links(),
            [Link {
                dataset: "ADSL",
                layer: 0,
                written: "adsl.yaml",
                from_entry: false,
            }]
        );
        assert!(!copies.resolved.model);
        assert!(copies.resolved.published.contains("AGE"));
        // A later layer that clears the link leaves the input unlinked.
        let unlinked =
            T::map(vec![("clear", T::list(vec![T::text("input.ADSL.schema")]))]).document();
        let copies = Copies::of(&[&parent, &unlinked]).unwrap();
        let (layer, findings) = copies.dataset_findings().unwrap();
        assert_eq!(layer, 0);
        assert_eq!(findings[0].code, ConditionCode::CopyUnlinkedInput);
        assert_eq!(findings[0].path, "columns[0].copy.dataset");
        assert!(Copies::of(&[&child]).is_none());
    }
}
