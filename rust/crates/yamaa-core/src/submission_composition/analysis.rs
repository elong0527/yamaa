//! Analysis results metadata composed over a section's datasets (DEFINE-31,
//! DEFINE-32): every result display of the section's sources, in source and
//! declaration order, each reference resolved against the datasets the
//! section composes and the documents it declares. No data is read: the
//! checks are on names, types and terminology.
use super::{
    declared_column, reproduced, strings, text, unknown_documents, Dataset, Finding, Input,
};
use crate::{
    ast,
    dataset::WhereTerm,
    diagnostic::ConditionCode,
    json::Value as Json,
    names::Positions,
    project_terminology as ct, specification_submission_document as spec,
    submission_values::{self as values, Scope},
    value::{ColumnType, Value},
};
use std::collections::BTreeSet;

/// One composed result display: its declaration and its composed results.
#[derive(Clone, Debug)]
pub struct AnalysisDisplay {
    pub(crate) id: String,
    pub(crate) oid: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) documents: Vec<ast::DocumentRef>,
    pub(crate) results: Vec<AnalysisResult>,
    pub(crate) path: String,
}
impl AnalysisDisplay {
    /// The declared identifier, which opens the identities of the
    /// display's results (DEFINE-9).
    pub fn id(&self) -> &str {
        &self.id
    }
    /// The `arm:ResultDisplay` OID: the declared `oid`, otherwise
    /// `RD.<id>` (DEFINE-9).
    pub fn oid(&self) -> &str {
        &self.oid
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    /// Where the display is in the section's supporting documents.
    pub fn documents(&self) -> &[ast::DocumentRef] {
        &self.documents
    }
    pub fn results(&self) -> &[AnalysisResult] {
        &self.results
    }
    /// The display's spec path in the project.
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// One composed analysis result: its declaration, the datasets it reads and
/// the dataset whose `PARAMCD` its `ParameterOID` names.
#[derive(Clone, Debug)]
pub struct AnalysisResult {
    pub(crate) declaration: ast::AnalysisResult,
    pub(crate) oid: String,
    pub(crate) parameter: Option<usize>,
    pub(crate) datasets: Vec<AnalysisDataset>,
    pub(crate) path: String,
}
impl AnalysisResult {
    /// The declared identifier, which closes the identities of the
    /// result's where clauses and join comment (DEFINE-9).
    pub fn id(&self) -> &str {
        &self.declaration.id.name
    }
    /// The `arm:AnalysisResult` OID: the declared `oid`, otherwise
    /// `AR.<display id>.<id>` (DEFINE-9).
    pub fn oid(&self) -> &str {
        &self.oid
    }
    pub fn description(&self) -> &str {
        &self.declaration.description.text
    }
    pub fn reason(&self) -> &str {
        &self.declaration.reason.text
    }
    pub fn purpose(&self) -> &str {
        &self.declaration.purpose.text
    }
    /// The position, among the study's datasets, of the first dataset the
    /// result lists that outputs `PARAMCD`; none when no listed dataset does
    /// (DEFINE-32).
    pub fn parameter(&self) -> Option<usize> {
        self.parameter
    }
    /// The datasets the result reads, in written order.
    pub fn datasets(&self) -> &[AnalysisDataset] {
        &self.datasets
    }
    /// How the datasets are joined, declared exactly when there are several.
    pub fn join(&self) -> Option<&ast::Comment> {
        self.declaration.join.as_ref()
    }
    pub fn documentation(&self) -> Option<&ast::Comment> {
        self.declaration.documentation.as_ref()
    }
    pub fn programming_code(&self) -> Option<&ast::ProgrammingCode> {
        self.declaration.programming_code.as_ref()
    }
    /// The result's spec path in the project.
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// One dataset a composed result reads.
#[derive(Clone, Debug)]
pub struct AnalysisDataset {
    pub(crate) dataset: usize,
    pub(crate) clause: Option<Vec<AnalysisTerm>>,
    pub(crate) variables: Vec<String>,
}
impl AnalysisDataset {
    /// The dataset's position among the study's datasets.
    pub fn dataset(&self) -> usize {
        self.dataset
    }
    /// The where clause, one term per where column in written order; none
    /// when the dataset declares no clause.
    pub fn clause(&self) -> Option<&[AnalysisTerm]> {
        self.clause.as_deref()
    }
    /// The analysed columns, in written order.
    pub fn variables(&self) -> &[String] {
        &self.variables
    }
}

/// One where column of an analysis dataset: its comparison, and its values
/// as the where column's type holds them when composition converted them.
#[derive(Clone, Debug)]
pub struct AnalysisTerm {
    pub(crate) term: WhereTerm,
    pub(crate) typed: Vec<Value>,
}
impl AnalysisTerm {
    /// The where column's position in its dataset and its comparison.
    pub fn term(&self) -> &WhereTerm {
        &self.term
    }
    /// Each value as the where column's type holds it, in written order: an
    /// ordered term's one value, and each value listed for a column that is
    /// not `str`. Empty for a `str` column, and for a reproduced dataset's
    /// listed values, which are written as declared (DEFINE-30).
    pub fn typed(&self) -> &[Value] {
        &self.typed
    }
}

/// What composing analysis results reads: the section's key, the project's
/// terminology, the documents the section declares, and each listed
/// specification with the dataset it composed.
pub(super) struct Context<'a> {
    pub(super) section: &'a str,
    pub(super) catalogue: &'a ct::Catalogue,
    pub(super) known: &'a BTreeSet<&'a str>,
    pub(super) inputs: &'a [Input<'a>],
    pub(super) datasets: &'a [Dataset],
}

/// One listed specification as an analysis result reads it: its dataset id,
/// whether its metadata is reproduced, its declared columns and the columns
/// it outputs.
struct Listed {
    id: String,
    reproduced: bool,
    declarations: Option<spec::Declarations>,
    output: Vec<String>,
}
impl Listed {
    fn of(input: &Input<'_>) -> Self {
        let document = input.document;
        let root = document.root();
        Self {
            id: text(document, root, "domain").unwrap_or_default(),
            reproduced: input
                .tree
                .submission
                .as_ref()
                .is_some_and(|metadata| metadata.mode == spec::SubmissionMode::Reproduced),
            declarations: spec::decode(document, root).ok(),
            output: document
                .field(root, "output")
                .map_or_else(Vec::new, |output| strings(document, output, "columns")),
        }
    }
    fn columns(&self) -> &[spec::Column] {
        self.declarations
            .as_ref()
            .and_then(spec::Declarations::columns)
            .map_or(&[], |columns| columns.value.as_slice())
    }
    /// Whether the document defines the column `name`: an output column
    /// that reproduced metadata does not leave out (DEFINE-30).
    fn defines(&self, name: &str) -> bool {
        self.output.iter().any(|column| column == name)
            && !(self.reproduced
                && self
                    .columns()
                    .iter()
                    .any(|column| column.name.value == name && reproduced::omits(column)))
    }
}

fn string(value: &str) -> Json {
    Json::String(value.into())
}

/// A list or field an analysis results declaration needs and lacks.
fn required(path: &str, field: &str) -> Finding {
    Finding::cause(ConditionCode::AnalysisRequiredField, path, field).with("field", field)
}

/// Compose every display of the section's analysis results `sources`
/// (DEFINE-32), reporting each declaration composition refuses.
pub(super) fn compose(
    context: &Context<'_>,
    sources: &[ast::AnalysisResults],
    findings: &mut Vec<Finding>,
) -> Vec<AnalysisDisplay> {
    if sources.is_empty() {
        return Vec::new();
    }
    let listed: Vec<Listed> = context.inputs.iter().map(Listed::of).collect();
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut displays = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let at = format!("{}.analysis_results[{index}]", context.section);
        if source.displays.is_empty() {
            findings.push(required(&format!("{at}.displays"), "displays"));
        }
        for (index, display) in source.displays.iter().enumerate() {
            let path = format!("{at}.displays[{index}]");
            // A display's `id` and `name` each identify it among every
            // display of the section, whichever source declares it.
            for (field, value, seen, code) in [
                (
                    "id",
                    display.id.name.as_str(),
                    &mut ids,
                    ConditionCode::SubmissionDuplicateIdentifier,
                ),
                (
                    "name",
                    display.name.text.as_str(),
                    &mut names,
                    ConditionCode::AnalysisDuplicateName,
                ),
            ] {
                if !seen.insert(value) {
                    findings.push(
                        Finding::cause(code, &format!("{path}.{field}"), value)
                            .with("identifier", value),
                    );
                }
            }
            unknown_documents(
                findings,
                display
                    .documents
                    .iter()
                    .map(|reference| reference.document.name.as_str()),
                context.known,
                &format!("{path}.documents"),
            );
            if display.results.is_empty() {
                findings.push(required(&format!("{path}.results"), "results"));
            }
            let mut results = Vec::new();
            for (index, result) in display.results.iter().enumerate() {
                let result_path = format!("{path}.results[{index}]");
                let id = result.id.name.as_str();
                let first = findings.len();
                if display.results[..index]
                    .iter()
                    .any(|earlier| earlier.id.name == id)
                {
                    findings.push(
                        Finding::cause(
                            ConditionCode::AnalysisDuplicateResult,
                            &format!("{result_path}.id"),
                            id,
                        )
                        .with("identifier", id),
                    );
                }
                let site = Site {
                    display: &display.id.name,
                    result,
                    path: &result_path,
                };
                let datasets = compose_result(context, &listed, &site, findings);
                // A result composition refuses has no definition; the whole
                // composition then fails with its findings.
                match datasets {
                    Some((parameter, datasets)) if findings.len() == first => {
                        results.push(AnalysisResult {
                            declaration: result.clone(),
                            oid: result.oid.as_ref().map_or_else(
                                || format!("AR.{}.{id}", display.id.name),
                                |oid| oid.text.clone(),
                            ),
                            parameter,
                            datasets,
                            path: result_path,
                        });
                    }
                    // A result is never left out of a composition that
                    // succeeds: one whose datasets have no definition while
                    // nothing was refused shows this check and the composed
                    // datasets disagreeing, and fails here.
                    None if findings.is_empty() => {
                        findings.push(crate::define_diagnostics::xml_invalid(
                            &result_path,
                            "an analysis result's datasets have no definition",
                            string(id),
                        ));
                    }
                    _ => {}
                }
            }
            displays.push(AnalysisDisplay {
                id: display.id.name.clone(),
                oid: display
                    .oid
                    .as_ref()
                    .map_or_else(|| format!("RD.{}", display.id.name), |oid| oid.text.clone()),
                name: display.name.text.clone(),
                description: display.description.text.clone(),
                documents: display.documents.clone(),
                results,
                path,
            });
        }
    }
    displays
}

/// One result under composition: its display's id, its declaration and its
/// spec path.
struct Site<'a> {
    display: &'a str,
    result: &'a ast::AnalysisResult,
    path: &'a str,
}
impl Site<'_> {
    /// A fault of the analysis dataset `dataset` at `path`, naming the
    /// display, the result, the dataset and the `reason` (DEFINE-32).
    fn invalid(&self, path: &str, dataset: &str, reason: &str) -> Finding {
        Finding::cause(ConditionCode::AnalysisDatasetInvalid, path, reason)
            .with("display", self.display)
            .with("result", self.result.id.name.as_str())
            .with("dataset", dataset)
            .with("reason", reason)
    }
}

/// Check one result's declarations and compose the datasets it reads: the
/// dataset whose `PARAMCD` its `ParameterOID` names and each listed dataset.
/// `None` when a dataset cannot be composed, which a finding explains.
fn compose_result(
    context: &Context<'_>,
    listed: &[Listed],
    site: &Site<'_>,
    findings: &mut Vec<Finding>,
) -> Option<(Option<usize>, Vec<AnalysisDataset>)> {
    let Site { result, path, .. } = *site;
    if result.datasets.is_empty() {
        findings.push(required(&format!("{path}.datasets"), "datasets"));
    }
    // A comment says how several datasets are joined; one dataset has
    // nothing to join.
    match (&result.join, result.datasets.len()) {
        (None, count) if count >= 2 => findings.push(required(&format!("{path}.join"), "join")),
        (Some(join), 1) => findings.push(
            Finding::cause(
                ConditionCode::AnalysisJoinNotPermitted,
                &format!("{path}.join"),
                "a result that reads one dataset joins nothing",
            )
            .with("value", join.text.text.as_str()),
        ),
        _ => {}
    }
    let named = |documents: &[ast::DocumentRef]| -> Vec<String> {
        documents
            .iter()
            .map(|reference| reference.document.name.clone())
            .collect()
    };
    let code = result.programming_code.as_ref();
    for (field, documents) in [
        ("join", result.join.as_ref().map(|join| &join.documents)),
        (
            "documentation",
            result.documentation.as_ref().map(|text| &text.documents),
        ),
        ("programming_code", code.map(|code| &code.documents)),
    ] {
        let documents = documents.map_or_else(Vec::new, |documents| named(documents));
        unknown_documents(
            findings,
            documents.iter().map(String::as_str),
            context.known,
            &format!("{path}.{field}"),
        );
    }
    if let Some(code) = code {
        let at = format!("{path}.programming_code");
        // Code is stated as text, as the documents that hold it, or both,
        // and text names the language it is written in.
        if code.code.is_none() && code.documents.is_empty() {
            findings.push(required(&at, "code"));
        }
        if code.code.is_some() && code.context.is_none() {
            findings.push(required(&format!("{at}.context"), "context"));
        }
    }
    let mut composed = Vec::new();
    let mut complete = true;
    for dataset in &result.datasets {
        match compose_dataset(context, listed, site, dataset, findings) {
            Some(dataset) => composed.push(dataset),
            None => complete = false,
        }
    }
    if !result.datasets.is_empty()
        && result
            .datasets
            .iter()
            .all(|dataset| dataset.variables.is_empty())
    {
        findings.push(required(&format!("{path}.datasets"), "variables"));
    }
    // The parameter is the first listed dataset's whose document defines
    // `PARAMCD`.
    let parameter = result.datasets.iter().find_map(|dataset| {
        let name = dataset.dataset.name.as_str();
        listed
            .iter()
            .find(|listed| listed.id == name)
            .filter(|listed| listed.defines("PARAMCD"))
            .and_then(|_| {
                context
                    .datasets
                    .iter()
                    .position(|composed| composed.id == name)
            })
    });
    complete.then_some((parameter, composed))
}

/// Check one dataset a result reads against the dataset the section
/// composes under its key: its variables, then its where clause through the
/// checker a value-level entry's clause takes (SUB-12).
fn compose_dataset(
    context: &Context<'_>,
    listed: &[Listed],
    site: &Site<'_>,
    dataset: &ast::AnalysisDataset,
    findings: &mut Vec<Finding>,
) -> Option<AnalysisDataset> {
    let name = dataset.dataset.name.as_str();
    let path = format!("{}.datasets.{name}", site.path);
    let Some(listed) = listed.iter().find(|listed| listed.id == name) else {
        let composed = context
            .inputs
            .iter()
            .filter_map(|input| text(input.document, input.document.root(), "domain"))
            .map(Json::String)
            .collect::<Vec<_>>();
        findings.push(
            Finding::cause(ConditionCode::AnalysisUnknownDataset, &path, name)
                .with("dataset", name)
                .with("section", context.section)
                .with("datasets", composed),
        );
        return None;
    };
    let first = findings.len();
    let columns = listed.columns();
    // Each list is looked up by name, never searched (#4782).
    let declared = Positions::of(columns.iter().map(|column| column.name.value.as_str()));
    let output = Positions::of(listed.output.iter().map(String::as_str));
    let variables = Positions::of(dataset.variables.iter().map(|v| v.name.as_str()));
    for (index, variable) in dataset.variables.iter().enumerate() {
        let variable = variable.name.as_str();
        let reason = if variables.repeats(index, variable) {
            "duplicate_variable"
        } else if !declared.contains(variable) {
            "undeclared"
        } else if !output.contains(variable) {
            "not_output"
        } else if !listed.defines(variable) {
            "omitted"
        } else {
            continue;
        };
        findings.push(
            site.invalid(&format!("{path}.variables[{index}]"), name, reason)
                .with("variable", variable),
        );
    }
    let scope = if listed.reproduced {
        Scope::Reproduced
    } else {
        Scope::Analysis
    };
    let clause = dataset.conditions.as_deref().map(values::clause_of);
    if let Some(clause) = &clause {
        let written = std::iter::once((site.result.id.name.as_str(), clause));
        // An analysis dataset describes no column of its own, so no where
        // column is the clause's own.
        for finding in values::admit(written, "", scope, |column| {
            declared_column(
                columns,
                (&declared, &output),
                context.catalogue,
                listed.reproduced,
                column,
            )
        }) {
            let values::Finding::Where { column, fault, .. } = finding else {
                continue;
            };
            let at = column.map_or_else(
                || format!("{path}.where"),
                |column| format!("{path}.where.{column}"),
            );
            let mut refused = site
                .invalid(&at, name, fault.reason())
                .with("where_column", column.map_or(Json::Null, string));
            if let Some(values) = fault.values() {
                let values: Vec<_> = values.iter().map(|value| string(value)).collect();
                refused = refused.with("values", values);
            }
            if let Some(comparator) = fault.comparator() {
                refused = refused.with("comparator", comparator);
            }
            findings.push(refused);
        }
    }
    if findings.len() != first {
        return None;
    }
    let position = context
        .datasets
        .iter()
        .position(|composed| composed.id == name)?;
    let composed = &context.datasets[position];
    let clause = match clause {
        Some(clause) => Some(
            clause
                .iter()
                .map(|term| analysis_term(composed, term, scope))
                .collect::<Option<Vec<_>>>()?,
        ),
        None => None,
    };
    Some(AnalysisDataset {
        dataset: position,
        clause,
        variables: dataset
            .variables
            .iter()
            .map(|variable| variable.name.clone())
            .collect(),
    })
}

/// One admitted where column of an analysis dataset over the composed
/// dataset's columns: its executable comparison, built as a value-level
/// entry's is, and its values in the where column's type.
fn analysis_term(
    composed: &Dataset,
    term: &values::Term<'_>,
    scope: Scope,
) -> Option<AnalysisTerm> {
    let index = composed
        .columns
        .iter()
        .position(|column| column.declaration.name.value == term.column)?;
    let kind = composed.columns[index].runtime_type();
    let written: Vec<String> = term
        .values
        .iter()
        .map(|value| (*value).to_owned())
        .collect();
    let term = values::where_term(index, term.comparator, written, kind)?;
    let typed = match &term.bound {
        Some(bound) => vec![bound.value.clone()],
        None if scope == Scope::Analysis && kind != ColumnType::Str => term
            .values
            .iter()
            .map(|value| values::converted(value, kind))
            .collect::<Option<Vec<_>>>()?,
        None => Vec::new(),
    };
    Some(AnalysisTerm { term, typed })
}
