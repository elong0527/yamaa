//! The findings that read a 1.x file through its schema: the canonical
//! spellings `redundant_long_form`, `restated_default` and `scalar_as_text`
//! (P-LONG-1, P-HND-3, P-KIND-1), and `equality_filter` (P-ORD-3), which
//! reads the lookup filters the schema locates.
//!
//! Each reads the file as its schema does. The engine's own validation walk
//! ([`SchemaStructure::written_sites`]) reports, for each written value, the
//! class field it fills, the named type it is read as, or the long form it
//! restates, under the structure validation chooses for the document. The
//! lint matches those values to the nodes it locates by walking the decoded
//! values and its own nodes together, since both come from one scan.
//!
//! Each canonical finding names a rewrite to the canonical spelling, which
//! keeps the value's meaning but not its written value: a restated default
//! is removed, grammar text for a missing value, number or Boolean becomes
//! that YAML scalar, and a long form becomes its shorthand. [`same_meaning`]
//! proves such a rewrite: the rewritten file must read as exactly the
//! original's values with that one change.

use yamaa_core::expression_table::Verb;
use yamaa_core::language::{FileType, LanguageVersion, LANGUAGE_VERSION};
use yamaa_core::schema::{
    Document as Values, DocumentNode as N, SchemaAliasKind, SchemaStructure, SiteRole, WrittenSite,
};

use super::checks::{Departure, Rewrite};
use super::document::{Document, Style, Value};

/// The handlers whose omission answers with the missing value (COLUMN-10),
/// by the class or operation that declares them, so YAML null restates them.
/// A handler whose omission fails, such as `unmapped` or `unconvertible`,
/// declares an answer when it is null.
const HANDLERS: [(&str, &str); 14] = [
    ("source_binding_class", "absent"),
    ("intermediate_class", "absent"),
    (Verb::FirstAvailable.name(), "missing"),
    ("flag_class", "false_value"),
    ("flag_class", "missing"),
    (Verb::Mapping.name(), "missing"),
    (Verb::Decode.name(), "missing"),
    (Verb::Cut.name(), "missing"),
    (Verb::StrExtract.name(), "missing"),
    (Verb::StrReplace.name(), "missing"),
    (Verb::StrCase.name(), "missing"),
    (Verb::StrSubstr.name(), "missing"),
    (Verb::StrLength.name(), "missing"),
    (Verb::StrDecimal.name(), "missing"),
];

/// The language version and file type a 1.x document declares, when the
/// engine reads them.
fn header(values: &Values) -> Option<(LanguageVersion, FileType)> {
    let root = values.root();
    let text = |name| match &values.nodes()[values.field(root, name)?] {
        N::Text(text) => Some(text.as_str()),
        _ => None,
    };
    let version = LanguageVersion::parse(text("yamaa")?).filter(|v| *v <= LANGUAGE_VERSION)?;
    Some((version, FileType::parse(text("type")?)?))
}

/// The schema view a 1.x document is read against, and whether it is a
/// specification, which is read as one layer.
fn view(values: &Values) -> Option<(SchemaStructure, bool)> {
    let (version, file_type) = header(values)?;
    let captured = crate::shipped_schema::capture_versioned(file_type).ok()?;
    let view = captured.structure().at_version(version);
    Some((view, file_type == FileType::Spec))
}

/// The lint's node for each decoded node, walking both trees together; none
/// when they differ, which one scan never gives.
pub(super) fn correspond(document: &Document, values: &Values) -> Option<Vec<usize>> {
    fn walk(
        document: &Document,
        values: &Values,
        node: usize,
        value: usize,
        nodes: &mut [usize],
    ) -> bool {
        nodes[value] = node;
        match (&document.node(node).value, &values.nodes()[value]) {
            (Value::Mapping { pairs, .. }, N::Mapping(items)) => {
                pairs.len() == items.len()
                    && pairs.iter().zip(items).all(|(&(key, item), &(k, v))| {
                        walk(document, values, key, k, nodes)
                            && walk(document, values, item, v, nodes)
                    })
            }
            (Value::Sequence { items, .. }, N::Sequence(entries)) => {
                items.len() == entries.len()
                    && items
                        .iter()
                        .zip(entries)
                        .all(|(&item, &entry)| walk(document, values, item, entry, nodes))
            }
            (Value::Scalar { .. }, N::Mapping(_) | N::Sequence(_)) => false,
            (Value::Scalar { .. }, _) => true,
            _ => false,
        }
    }
    let mut nodes = vec![usize::MAX; values.nodes().len()];
    walk(document, values, document.root?, values.root(), &mut nodes).then_some(nodes)
}

/// Typed equality across two documents: text with text, integers by value,
/// floats by value, and collections member by member.
pub(super) fn same(first: &Values, a: usize, second: &Values, b: usize) -> bool {
    match (&first.nodes()[a], &second.nodes()[b]) {
        (N::Sequence(x), N::Sequence(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(&a, &b)| same(first, a, second, b))
        }
        (N::Mapping(x), N::Mapping(y)) => {
            x.len() == y.len()
                && x.iter().all(|&(key, value)| {
                    y.iter().any(|&(other, item)| {
                        same(first, key, second, other) && same(first, value, second, item)
                    })
                })
        }
        (x, y) => x == y,
    }
}

/// One 1.x file read through its schema.
struct Read<'a> {
    document: &'a Document,
    values: &'a Values,
    /// The lint's node for each decoded node.
    nodes: Vec<usize>,
    view: SchemaStructure,
    sites: Vec<WrittenSite>,
    /// The key that names each lint node held as a mapping value, and the
    /// mapping that holds it.
    keys: Vec<Option<(usize, usize)>>,
}

impl<'a> Read<'a> {
    fn new(document: &'a Document) -> Option<Self> {
        let values = &document.decoded.as_ref()?.document;
        if !yamaa_core::language::declares_header(values) {
            return None;
        }
        let (view, layer) = view(values)?;
        let nodes = correspond(document, values)?;
        let sites = view.written_sites(values, layer).ok()?;
        let mut keys = vec![None; document.nodes.len()];
        for (id, node) in document.nodes.iter().enumerate() {
            if let Value::Mapping { pairs, .. } = &node.value {
                for &(key, value) in pairs {
                    keys[value] = Some((key, id));
                }
            }
        }
        Some(Self {
            document,
            values,
            nodes,
            view,
            sites,
            keys,
        })
    }

    /// The default a descriptor declares, with the module document it is
    /// written in.
    fn default(&self, descriptor: usize) -> Option<(&Values, usize)> {
        let located = &self.view.descriptors()[descriptor];
        let node = located.descriptor.default_node()?;
        Some((&self.view.modules()[located.module].document, node))
    }

    /// `restated_default`: a field or handler that writes its default. A
    /// field writes the default its schema declares, and a handler whose
    /// omission answers with the missing value writes YAML null, except a
    /// flag's `missing` beside a declared `false_value`, which requires it
    /// (EXPR-8). A layer that declares `parent` or `include`, or that another
    /// linted file includes, is left out: a value it writes may replace an
    /// inherited one.
    fn restated_defaults(&self) -> Vec<Departure> {
        let mut found = Vec::new();
        if self.document.layered() {
            return found;
        }
        for site in &self.sites {
            let SiteRole::Field {
                owner,
                field,
                descriptor,
            } = &site.role
            else {
                continue;
            };
            let value = self.nodes[site.node];
            let Some((key, mapping)) = self.keys[value] else {
                continue;
            };
            let declares = |name: &str| self.document.member(Some(mapping), name);
            let restates = match self.default(*descriptor) {
                Some((module, default)) => same(self.values, site.node, module, default),
                None => {
                    matches!(self.values.nodes()[site.node], N::Null)
                        && HANDLERS.contains(&(owner.as_str(), field.as_str()))
                        && !(owner == "flag_class"
                            && field == "missing"
                            && declares("false_value").is_some())
                        // An absence answer beside `require_match: true`
                        // conflicts with it, which validation refuses
                        // (LOOKUP-24).
                        && !(field == "absent"
                            && declares("require_match").is_some_and(|flag| {
                                matches!(
                                    &self.document.node(flag).value,
                                    Value::Scalar { text, style: Style::Plain }
                                        if super::authoring::plain_true(text)
                                )
                            }))
                }
            };
            if !restates {
                continue;
            }
            let start = self.document.node(key).start;
            found.push(
                Departure::new(
                    "restated_default",
                    start.line,
                    start.column,
                    self.document.path(value),
                    format!(
                        "`{field}: {}` restates the default; remove it",
                        self.document.source(value)
                    ),
                )
                .with_fix(Rewrite::Remove { key }),
            );
        }
        found
    }

    /// Whether the named type `name` is an expression field that admits a
    /// YAML scalar beside grammar text: missing, a number or a Boolean.
    fn admits_scalar(&self, name: &str) -> bool {
        let Some((_, alias)) = self.view.alias_named(name) else {
            return false;
        };
        let SchemaAliasKind::Descriptor(descriptor) = alias.kind else {
            return false;
        };
        let descriptor = &self.view.descriptors()[descriptor].descriptor;
        descriptor.kind() == Some("expression")
            && descriptor.members().iter().any(|member| {
                matches!(
                    member.node_text(member.root()),
                    Some("null" | "int" | "float" | "bool")
                )
            })
    }

    /// `scalar_as_text`: an expression field whose whole value is grammar
    /// text for a missing value, a number or a Boolean, which the YAML scalar
    /// spells canonically (P-KIND-1).
    fn scalar_texts(&self) -> Vec<Departure> {
        let mut found = Vec::new();
        let mut seen = Vec::new();
        for site in &self.sites {
            let SiteRole::Alias { name } = &site.role else {
                continue;
            };
            let N::Text(text) = &self.values.nodes()[site.node] else {
                continue;
            };
            if seen.contains(&site.node) || !self.admits_scalar(name) {
                continue;
            }
            seen.push(site.node);
            let Some((scalar, what)) = scalar(text) else {
                continue;
            };
            let node = self.nodes[site.node];
            let start = self.document.node(node).start;
            found.push(
                Departure::new(
                    "scalar_as_text",
                    start.line,
                    start.column,
                    self.document.path(node),
                    format!(
                        "`{}` is grammar text for {what}; write the YAML scalar `{scalar}`",
                        self.document.source(node)
                    ),
                )
                .with_fix(Rewrite::Scalar { node, text: scalar }),
            );
        }
        found
    }

    /// `redundant_long_form`: a long form that holds only what its shorthand
    /// fills, which validation refuses (SCHEMA-15). The shorthand replaces
    /// the long form, or the whole operation whose payload it is.
    fn long_forms(&self) -> Vec<Departure> {
        let mut found = Vec::new();
        for site in &self.sites {
            let SiteRole::LongForm {
                field,
                shorthand,
                operation,
            } = &site.role
            else {
                continue;
            };
            let node = self.nodes[site.node];
            let replaced = operation.map_or(node, |operation| self.nodes[operation]);
            let shorthand = shorthand.map(|value| self.nodes[value]);
            let written = shorthand.map_or(field.as_str(), |value| self.document.source(value));
            let start = self.document.node(node).start;
            found.push(
                Departure::new(
                    "redundant_long_form",
                    start.line,
                    start.column,
                    self.document.path(node),
                    format!(
                        "the long form of `{field}` only restates its shorthand; write `{written}`"
                    ),
                )
                .with_fix(Rewrite::Shorthand {
                    node: replaced,
                    shorthand,
                    keyword: field.clone(),
                }),
            );
        }
        found
    }
}

impl Read<'_> {
    /// The text a lookup's sibling field `name` holds, beside the filter
    /// whose lint node is `filter`.
    fn sibling(&self, filter: usize, name: &str) -> Option<&str> {
        let (_, mapping) = self.keys[filter]?;
        let node = self.document.member(Some(mapping), name)?;
        self.document.text_scalar(node)
    }

    /// The dataset whose records a lookup's filter narrows: a named
    /// intermediate's `dataset`, a source binding's variable's qualifier, or
    /// the one dataset a qualified aggregate's expression reads.
    fn donor(&self, owner: &str, filter: usize) -> Option<String> {
        match owner {
            "intermediate_class" => self.sibling(filter, "dataset").map(str::to_owned),
            "source_binding_class" => self
                .sibling(filter, "variable")?
                .split_once('.')
                .map(|(dataset, _)| dataset.to_owned()),
            "aggregate_class" => {
                let parsed = yamaa_core::expression::parse(self.sibling(filter, "expr")?).ok()?;
                let mut datasets = parsed
                    .identifiers()
                    .filter_map(|name| name.split_once('.').map(|(dataset, _)| dataset))
                    .chain(parsed.star_datasets())
                    .collect::<Vec<_>>();
                datasets.sort_unstable();
                datasets.dedup();
                match datasets.as_slice() {
                    [dataset] => Some((*dataset).to_owned()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// `equality_filter`: a lookup filter conjunct that compares a donor
    /// field with a current-row value by equality, which is a key pair
    /// (P-ORD-3, LOOKUP-5). The filters of a named intermediate, a source
    /// binding and a qualified aggregate are read; a conjunct that compares a
    /// donor field with a constant stays in `filter`.
    fn equality_filters(&self) -> Vec<Departure> {
        use yamaa_core::expression::{Comparison, NodeKind, Parsed};
        let mut found = Vec::new();
        for site in &self.sites {
            let SiteRole::Field { owner, field, .. } = &site.role else {
                continue;
            };
            let N::Text(text) = &self.values.nodes()[site.node] else {
                continue;
            };
            let filter = self.nodes[site.node];
            if field != "filter" {
                continue;
            }
            let Some(donor) = self.donor(owner, filter) else {
                continue;
            };
            let Ok(parsed) = yamaa_core::expression::parse(text) else {
                continue;
            };
            fn conjuncts(parsed: &Parsed, node: usize, out: &mut Vec<usize>) {
                let node = parsed.ungroup(node);
                match parsed.nodes()[node].kind {
                    NodeKind::And(left, right) => {
                        conjuncts(parsed, left, out);
                        conjuncts(parsed, right, out);
                    }
                    _ => out.push(node),
                }
            }
            // The names one operand reads, each with its qualifier.
            fn names<'p>(parsed: &'p Parsed, node: usize, out: &mut Vec<&'p str>) {
                match &parsed.nodes()[node].kind {
                    NodeKind::Identifier | NodeKind::Star => out.push(parsed.text(node)),
                    NodeKind::Group(inner) | NodeKind::Not(inner) => names(parsed, *inner, out),
                    NodeKind::Unary { operand, .. } => names(parsed, *operand, out),
                    NodeKind::Binary { left, right, .. }
                    | NodeKind::And(left, right)
                    | NodeKind::Or(left, right)
                    | NodeKind::Compare { left, right, .. } => {
                        names(parsed, *left, out);
                        names(parsed, *right, out);
                    }
                    NodeKind::Call { arguments, .. } => {
                        arguments.iter().for_each(|&item| names(parsed, item, out))
                    }
                    NodeKind::Reduction { operand, .. } => names(parsed, *operand, out),
                    _ => {}
                }
            }
            let qualifier =
                |name: &str| name.split_once('.').map(|(dataset, _)| dataset.to_owned());
            let donor_field = |node: usize| {
                let node = parsed.ungroup(node);
                parsed.nodes()[node].kind == NodeKind::Identifier
                    && qualifier(parsed.text(node)).as_deref() == Some(donor.as_str())
            };
            // A current-row value reads at least one name, each qualified by
            // a dataset other than the donor.
            let current_row = |node: usize| {
                let mut read = Vec::new();
                names(&parsed, node, &mut read);
                !read.is_empty()
                    && read.iter().all(|name| {
                        qualifier(name).is_some_and(|dataset| dataset != donor && dataset != "SELF")
                    })
            };
            let mut found_here = Vec::new();
            conjuncts(&parsed, parsed.root(), &mut found_here);
            for conjunct in found_here {
                let NodeKind::Compare {
                    operator: Comparison::Equal,
                    left,
                    right,
                } = parsed.nodes()[conjunct].kind
                else {
                    continue;
                };
                if !(donor_field(left) && current_row(right)
                    || donor_field(right) && current_row(left))
                {
                    continue;
                }
                let start = self.document.node(filter).start;
                found.push(Departure::new(
                    "equality_filter",
                    start.line,
                    start.column,
                    self.document.path(filter),
                    format!(
                        "`{}` compares a donor field with a current-row value; declare the \
                         pair in `key`",
                        parsed.text(conjunct)
                    ),
                ));
            }
        }
        found
    }
}

/// The YAML scalar that grammar `text` holding only a literal stands for, and
/// what it is: the missing value, a number or a Boolean. The scalar must read
/// back as the same value, so a number the YAML core schema reads otherwise
/// keeps its text.
fn scalar(text: &str) -> Option<(String, &'static str)> {
    use yamaa_core::expression::NodeKind;
    let parsed = yamaa_core::expression::parse(text).ok()?;
    let literal = &parsed.nodes()[parsed.ungroup(parsed.root())].kind;
    let (scalar, what) = match literal {
        NodeKind::Null => ("null".to_owned(), "the missing value"),
        NodeKind::Boolean(value) => (value.to_string(), "a Boolean"),
        NodeKind::Number { text, .. } => (text.clone(), "a number"),
        _ => return None,
    };
    let decoded = crate::yaml_decode::decode_yaml(scalar.as_bytes()).ok()?;
    let read = &decoded.document.nodes()[decoded.document.root()];
    let same = match (literal, read) {
        (NodeKind::Null, N::Null) => true,
        (NodeKind::Boolean(value), N::Boolean(read)) => value == read,
        (
            NodeKind::Number {
                fractional: false,
                text,
            },
            N::Integer(read),
        ) => text
            .parse::<i64>()
            .ok()
            .is_some_and(|value| read.parse() == Ok(value)),
        (
            NodeKind::Number {
                fractional: true,
                text,
            },
            N::Float(read),
        ) => text.parse::<f64>() == Ok(*read),
        _ => false,
    };
    same.then_some((scalar, what))
}

/// Every canonical-spelling departure in a 1.x `document`; none when the
/// engine reads no schema for it.
pub(super) fn check(document: &Document) -> Vec<Departure> {
    let Some(read) = Read::new(document) else {
        return Vec::new();
    };
    let mut found = read.long_forms();
    found.extend(read.restated_defaults());
    found.extend(read.scalar_texts());
    found.extend(read.equality_filters());
    found
}

/// The values `document` holds with `rewrite` applied, rebuilt as a new
/// document: a removed entry is left out, a scalar replaced and a long form
/// replaced by its shorthand's value.
fn rewritten(document: &Document, rewrite: &Rewrite) -> Option<Values> {
    let values = &document.decoded.as_ref()?.document;
    let nodes = correspond(document, values)?;
    let decoded = |lint: usize| nodes.iter().position(|&node| node == lint);
    enum Change {
        Remove(usize),
        Replace(usize, N),
        Substitute(usize, usize),
    }
    let change = match rewrite {
        Rewrite::Remove { key } => Change::Remove(decoded(*key)?),
        Rewrite::Scalar { node, text } => {
            let read = crate::yaml_decode::decode_yaml(text.as_bytes()).ok()?;
            let scalar = read.document.nodes()[read.document.root()].clone();
            if matches!(scalar, N::Mapping(_) | N::Sequence(_)) {
                return None;
            }
            Change::Replace(decoded(*node)?, scalar)
        }
        Rewrite::Shorthand {
            node,
            shorthand: Some(shorthand),
            ..
        } => Change::Substitute(decoded(*node)?, decoded(*shorthand)?),
        Rewrite::Shorthand {
            node,
            shorthand: None,
            keyword,
        } => Change::Replace(decoded(*node)?, N::Text(keyword.clone())),
    };
    fn copy(values: &Values, node: usize, change: &Change, out: &mut Vec<N>) -> usize {
        match change {
            Change::Replace(target, scalar) if *target == node => {
                out.push(scalar.clone());
                return out.len() - 1;
            }
            Change::Substitute(target, shorthand) if *target == node => {
                return copy(values, *shorthand, change, out);
            }
            _ => {}
        }
        let copied = match &values.nodes()[node] {
            N::Sequence(items) => N::Sequence(
                items
                    .iter()
                    .map(|&item| copy(values, item, change, out))
                    .collect(),
            ),
            N::Mapping(pairs) => N::Mapping(
                pairs
                    .iter()
                    .filter(
                        |&&(key, _)| !matches!(change, Change::Remove(removed) if *removed == key),
                    )
                    .map(|&(key, value)| {
                        (
                            copy(values, key, change, out),
                            copy(values, value, change, out),
                        )
                    })
                    .collect(),
            ),
            scalar => scalar.clone(),
        };
        out.push(copied);
        out.len() - 1
    }
    let mut out = Vec::with_capacity(values.nodes().len());
    let root = copy(values, values.root(), &change, &mut out);
    Values::new(out, root).ok()
}

/// True when `after` reads as exactly the values of `before` with `rewrite`
/// applied, both admitted by the engine's own document admission.
pub(super) fn same_meaning(before: &Document, rewrite: &Rewrite, after: &str) -> bool {
    let Some(expected) = rewritten(before, rewrite) else {
        return false;
    };
    crate::yaml_decode::decode_yaml(after.as_bytes())
        .is_ok_and(|read| expected.same_style_value(&read.document))
}

/// The schema findings validation gives a 1.x file, by spec path and
/// condition; none for a file the engine reads no schema for.
pub(super) fn refusals(
    text: &str,
) -> Option<std::collections::BTreeSet<(String, yamaa_core::diagnostic::ConditionCode)>> {
    use yamaa_core::schema::NormalizationError;
    let decoded = crate::yaml_decode::decode_yaml(text.as_bytes()).ok()?;
    let values = &decoded.document;
    if !yamaa_core::language::declares_header(values) {
        return None;
    }
    let (view, layer) = view(values)?;
    let found = if layer {
        view.validate_layer(values).ok()?
    } else {
        match view.normalize_root(values) {
            Ok(_) => Vec::new(),
            Err(NormalizationError::Invalid(found)) => found,
            Err(_) => return None,
        }
    };
    Some(
        found
            .into_iter()
            .map(|item| (item.path, item.code))
            .collect(),
    )
}
