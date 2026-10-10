//! Complete static graph diagnostics borrowed from sealed metadata ownership.
//! Projection refusal retains the exact original preparation error beside it.
use crate::{file_graph::FileGraph, file_workflow, issue_rows::Issue};
use yamaa_core::{
    diagnostic::{ContextValue, Diagnostic},
    specification::{PrepareError, VerificationDeclarationFinding as Finding},
    value::Value,
};

#[derive(Debug)]
pub enum Error<'a> {
    Original(&'a file_workflow::Error),
    Projection(crate::specification_check::Error),
}
pub(crate) struct Budget(crate::report_projection::Budget);
impl Budget {
    fn new(maximum: usize) -> Self {
        Self(crate::report_projection::Budget::new(maximum))
    }
    fn charge<T>(
        &mut self,
        f: impl FnOnce(
            &mut crate::report_projection::Budget,
        ) -> Result<T, crate::specification_report::Error>,
    ) -> Result<T, Error<'static>> {
        f(&mut self.0).map_err(|_| Error::Projection(crate::specification_check::Error::Limit))
    }
    fn entries(&mut self, n: usize) -> Result<(), Error<'static>> {
        self.charge(|b| b.entries(n))
    }
    fn text(&mut self, text: &str) -> Result<(), Error<'static>> {
        // Two passes conservatively cover temporary diagnostic ownership and
        // both JSON escaping layers before the first clone/format operation.
        self.charge(|b| {
            b.text(text)?;
            b.text(text)
        })
    }
    fn diagnostic(&mut self, diagnostic: &Diagnostic) -> Result<(), Error<'static>> {
        self.charge(|b| {
            b.diagnostic(diagnostic)?;
            b.diagnostic(diagnostic)
        })
    }
    fn finding(&mut self, finding: Finding<'_>) -> Result<(), Error<'static>> {
        self.entries(16)?;
        match finding {
            Finding::Diagnostic(diagnostic) => self.diagnostic(diagnostic),
            Finding::Declaration {
                path,
                condition,
                requirement,
                reason,
            } => {
                for text in [path, condition, requirement, reason] {
                    self.text(text)?;
                }
                Ok(())
            }
        }
    }
    fn declaring_sources(
        &mut self,
        document: &crate::specification_source::PreparedDocument,
        finding: Finding<'_>,
    ) -> Result<(), Error<'static>> {
        if let Some(inherited) = document.inheritance() {
            self.text("declaring_sources")?;
            let mut result = Ok(());
            finding.visit_paths(|path| {
                if result.is_ok() {
                    result = self
                        .charge(|b| {
                            b.work(
                                inherited
                                    .provenance()
                                    .len()
                                    .saturating_mul(path.len().saturating_add(1)),
                            )
                        })
                        .and_then(|()| self.entries(1))
                        .and_then(|()| {
                            document
                                .declaring_source(path)
                                .map_or(Ok(()), |source| self.text(source))
                        });
                }
            });
            result?;
        }
        Ok(())
    }
    fn document(
        &mut self,
        document: &yamaa_core::schema::Document,
        repeats: usize,
    ) -> Result<(), Error<'static>> {
        use yamaa_core::schema::DocumentNode as N;
        self.entries(
            document
                .nodes()
                .len()
                .checked_mul(repeats)
                .ok_or_else(invalid)?,
        )?;
        for node in document.nodes() {
            if let N::Text(text) | N::Integer(text) = node {
                for _ in 0..repeats {
                    self.text(text)?;
                }
            }
        }
        Ok(())
    }
    fn schema_findings(
        &mut self,
        schema: &crate::specification_source::CapturedSchema,
        input: Option<&yamaa_core::schema::Document>,
        findings: &[yamaa_core::schema::SchemaDiagnostic],
    ) -> Result<(), Error<'static>> {
        self.entries(findings.len().checked_mul(16).ok_or_else(invalid)?)?;
        // Descriptor/input occurrence references can repeat. Charge their whole
        // immutable source arenas per finding before the existing resolver copies.
        for source in schema.sources() {
            self.text(&source.identity)?;
            let text = std::str::from_utf8(&source.bytes).map_err(|_| invalid())?;
            for _ in findings {
                self.text(text)?;
            }
        }
        if let Some(input) = input {
            self.document(input, findings.len())?;
        }
        for finding in findings {
            self.text(&finding.path)?;
            self.entries(finding.context.len())?;
            for (key, context) in &finding.context {
                self.text(key)?;
                if let yamaa_core::schema::SchemaContext::Text(text) = context {
                    self.text(text)?;
                }
            }
        }
        Ok(())
    }
    fn captured(
        &mut self,
        captured: &crate::specification_source::CapturedFindings,
    ) -> Result<(), Error<'static>> {
        for _ in captured.findings() {
            self.text(&captured.source().identity)?;
            self.text(&captured.source().identity)?;
        }
        self.schema_findings(captured.schema(), captured.document(), captured.findings())
    }
}
fn invalid() -> Error<'static> {
    Error::Projection(crate::specification_check::Error::InvalidContext)
}
fn issue(
    mut diagnostic: Diagnostic,
    source: Option<(&str, &str)>,
) -> Result<Issue, Error<'static>> {
    if let Some((source, entry)) = source {
        for (key, text) in [("source", source), ("entry", entry)] {
            if diagnostic
                .context
                .insert(key.into(), ContextValue::Scalar(Value::Str(text.into())))
                .is_some()
            {
                return Err(invalid());
            }
        }
    }
    Issue::from_core(diagnostic).map_err(|_| invalid())
}
fn complete(rows: Vec<Issue>, maximum: usize) -> Result<Vec<Issue>, Error<'static>> {
    if !crate::issue_rows::within_limit(
        &rows,
        maximum.min(crate::specification_check::MAX_ISSUE_BYTES),
    ) {
        return Err(Error::Projection(crate::specification_check::Error::Limit));
    }
    Ok(rows)
}
pub fn prepared(graph: &FileGraph) -> Result<Vec<Issue>, Error<'_>> {
    prepared_with_limit(graph, crate::specification_check::MAX_ISSUE_BYTES)
}
pub fn prepared_with_limit(graph: &FileGraph, maximum: usize) -> Result<Vec<Issue>, Error<'_>> {
    let checked = graph.graph().checked().metadata();
    let entry = graph.graph().nodes()[checked.root()]
        .document
        .source()
        .identity
        .as_str();
    let multiple = checked.nodes().len() > 1;
    let mut budget = Budget::new(maximum);
    for node in graph.graph().nodes() {
        budget.charge(|b| {
            b.work(
                node.document
                    .model()
                    .document()
                    .nodes()
                    .len()
                    .saturating_mul(2),
            )
        })?;
    }
    // Complete preflight precedes allocation of any diagnostic or issue. A
    // failing later node cannot turn an earlier fitting prefix into success.
    for (index, finding) in checked.check_findings() {
        budget.finding(finding)?;
        if multiple {
            budget.text(&graph.graph().nodes()[index].document.source().identity)?;
            budget.text(entry)?;
            budget.declaring_sources(&graph.graph().nodes()[index].document, finding)?;
        }
    }
    let rows = checked
        .check_findings()
        .map(|(index, finding)| {
            let mut diagnostic = finding.diagnostic();
            let document = &graph.graph().nodes()[index].document;
            if multiple && document.inheritance().is_some() {
                let origins = diagnostic
                    .spec_paths
                    .iter()
                    .map(|path| match document.declaring_source(path) {
                        Some(source) => ContextValue::Scalar(Value::Str(source.into())),
                        None => ContextValue::Scalar(Value::Missing),
                    })
                    .collect();
                if diagnostic
                    .context
                    .insert("declaring_sources".into(), ContextValue::Sequence(origins))
                    .is_some()
                {
                    return Err(invalid());
                }
            }
            issue(
                diagnostic,
                multiple.then(|| {
                    (
                        graph.graph().nodes()[index]
                            .document
                            .source()
                            .identity
                            .as_str(),
                        entry,
                    )
                }),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    complete(rows, maximum)
}

enum Rejection<'a> {
    Diagnostics(&'a [Diagnostic]),
    Compile(&'a PrepareError),
}
fn admission(error: &yamaa_core::producer_admission::Error) -> Option<Rejection<'_>> {
    match error {
        yamaa_core::producer_admission::Error::Invalid(diagnostics) => {
            Some(Rejection::Diagnostics(diagnostics))
        }
        yamaa_core::producer_admission::Error::Compilation(error) => {
            Some(Rejection::Compile(error))
        }
        _ => None,
    }
}
fn graph_error(error: &crate::producer_graph::Error) -> Option<(usize, Rejection<'_>)> {
    use crate::producer_graph::Error as A;
    use yamaa_core::producer_graph::Error as C;
    match error {
        A::Graph(C::Admission { node, error }) => Some((*node, admission(error)?)),
        A::Graph(C::Compilation { node, error }) => Some((*node, Rejection::Compile(error))),
        A::Graph(C::Cycle { node, diagnostic }) => Some((
            *node,
            Rejection::Diagnostics(std::slice::from_ref(diagnostic)),
        )),
        A::Metadata {
            node,
            error: crate::producer_admission::Error::Admission(error),
        } => Some((*node, admission(error)?)),
        _ => None,
    }
}
pub fn rejected(
    error: &file_workflow::Error,
    host: yamaa_core::project_function::Language,
) -> Result<Vec<Issue>, Error<'_>> {
    rejected_with_limit(error, host, crate::specification_check::MAX_ISSUE_BYTES)
}
pub fn rejected_with_limit(
    error: &file_workflow::Error,
    host: yamaa_core::project_function::Language,
    maximum: usize,
) -> Result<Vec<Issue>, Error<'_>> {
    if let file_workflow::Error::Environment(environment) = error {
        return environment_issues(error, environment.error(), host, maximum);
    }
    let file_workflow::Error::Graph(graph) = error else {
        return Err(Error::Original(error));
    };
    if let crate::file_graph::Error::Document(crate::file_preparation::Error::Preparation(cause)) =
        graph.error()
    {
        return inheritance_issues(error, cause, maximum);
    }
    let crate::file_graph::Error::Graph(cause) = graph.error() else {
        return Err(Error::Original(error));
    };
    let Some((index, rejection)) = graph_error(cause) else {
        return Err(Error::Original(error));
    };
    let source = graph
        .documents()
        .get(index)
        .ok_or_else(invalid)?
        .source()
        .identity
        .as_str();
    let entry = graph
        .documents()
        .first()
        .ok_or_else(invalid)?
        .source()
        .identity
        .as_str();
    let provenance = (graph.documents().len() > 1).then_some((source, entry));
    let mut budget = Budget::new(maximum);
    match rejection {
        Rejection::Diagnostics(diagnostics) => {
            if diagnostics.is_empty() {
                return Err(invalid());
            }
            for diagnostic in diagnostics {
                budget.diagnostic(diagnostic)?;
                budget.text(source)?;
                budget.text(entry)?;
            }
            complete(
                diagnostics
                    .iter()
                    .map(|d| issue(d.clone(), provenance))
                    .collect::<Result<Vec<_>, _>>()?,
                maximum,
            )
        }
        Rejection::Compile(PrepareError::Invalid(findings)) => {
            if findings.is_empty() {
                return Err(invalid());
            }
            for finding in findings {
                budget.entries(16)?;
                let mut result = Ok(());
                finding.visit_diagnostic_text(|text| {
                    if result.is_ok() {
                        result = budget.text(text);
                    }
                });
                result?;
                budget.text(source)?;
                budget.text(entry)?;
            }
            complete(
                findings
                    .iter()
                    .map(|f| issue(f.diagnostic(), provenance))
                    .collect::<Result<Vec<_>, _>>()?,
                maximum,
            )
        }
        Rejection::Compile(PrepareError::Unsupported(features)) => {
            if features.is_empty() {
                return Err(invalid());
            }
            for feature in features {
                budget.entries(16)?;
                budget.text(&feature.operation)?;
                budget.text(&feature.path)?;
                budget.text(source)?;
                budget.text(entry)?;
            }
            complete(
                features
                    .iter()
                    .map(|feature| {
                        issue(
                            yamaa_core::application_issue::unsupported(
                                &feature.operation,
                                Some(&feature.path),
                            ),
                            provenance,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                maximum,
            )
        }
        _ => Err(Error::Original(error)),
    }
}

fn environment_issues<'a>(
    owner: &'a file_workflow::Error,
    cause: &crate::file_project::Error,
    host: yamaa_core::project_function::Language,
    maximum: usize,
) -> Result<Vec<Issue>, Error<'a>> {
    use crate::project_environment_diagnostics as D;
    use crate::project_source::{CaptureFailure as C, Failure as F};
    use crate::specification_source::Error as S;
    use yamaa_core::project_limits::AdmissionError;
    let crate::file_project::Error::Environment(cause) = cause else {
        return Err(Error::Original(owner));
    };
    let mut budget = Budget::new(maximum);
    let mut issues = Vec::new();
    let projection = || invalid();
    match cause {
        F::Root(S::Findings(captured)) => {
            budget.captured(captured)?;
            issues.extend(D::schema_root(captured).map_err(|_| projection())?);
        }
        F::RootScalar { root, findings } => {
            for _ in 0..findings.len().saturating_add(8) {
                budget.text(&root.source().identity)?;
            }
            budget.document(
                &root.normalized().document,
                findings.len().saturating_add(8),
            )?;
            issues
                .extend(D::scalar_root_with_host(root, findings, host).map_err(|_| projection())?);
        }
        F::Rejected(rejected) => {
            let count = match &rejected.admission {
                Some(AdmissionError::Limit(_)) => return Err(Error::Original(owner)),
                Some(AdmissionError::Findings(findings)) => findings.len(),
                None => 0,
            };
            let count = rejected
                .sources
                .iter()
                .try_fold(count.saturating_add(8), |count, source| {
                    match &source.error {
                        C::Scalar { findings, .. } => count.checked_add(findings.len()),
                        C::Structural(S::Findings(captured)) => {
                            count.checked_add(captured.findings().len())
                        }
                        _ => None,
                    }
                })
                .ok_or(Error::Original(owner))?;
            // A complete typed result is projected only when every capture
            // cause is supported. Opaque/native/transport causes stay owned.
            budget.entries(count)?;
            for _ in 0..count {
                budget.text(&rejected.root.source().identity)?;
            }
            budget.document(&rejected.root.normalized().document, count)?;
            for capture in &rejected.captures {
                for _ in 0..count {
                    budget.text(&capture.document.source().identity)?;
                }
                budget.document(&capture.document.normalized().document, count)?;
            }
            for source in &rejected.sources {
                for _ in 0..count {
                    budget.text(&source.written)?;
                }
                match &source.error {
                    C::Scalar { document, .. } => {
                        for _ in 0..count {
                            budget.text(&document.source().identity)?;
                        }
                        budget.document(&document.normalized().document, count)?;
                    }
                    C::Structural(S::Findings(captured)) => budget.captured(captured)?,
                    _ => return Err(Error::Original(owner)),
                }
            }
            for rows in [
                D::admission(rejected),
                D::scalar_sources(rejected),
                D::schema_sources(rejected),
            ] {
                issues.extend(rows.map_err(|_| projection())?);
            }
        }
        _ => return Err(Error::Original(owner)),
    }
    complete(issues, maximum)
}

fn inheritance_issues<'a>(
    owner: &'a file_workflow::Error,
    cause: &crate::specification_source::InheritanceError<crate::file_resources::Error>,
    maximum: usize,
) -> Result<Vec<Issue>, Error<'a>> {
    use crate::specification_source::{Error as S, InheritanceError as I};
    use yamaa_core::schema::{
        InheritanceDependencyError as D, InheritanceDependencyIssue as F,
        InheritanceReferenceError as R, NormalizationError as N,
    };
    use yamaa_engine::{inheritance::Error as G, inheritance_preparation::Error as E};
    let mut budget = Budget::new(maximum);
    let mut supported = true;
    match cause {
        I::Entry(S::Findings(captured)) => budget.captured(captured)?,
        I::Entry(S::Decode { identity, error }) => {
            use crate::yaml_decode::DecodeFailure;
            budget.entries(16)?;
            budget.text(identity)?;
            match error {
                DecodeFailure::NonAscii(_) => (),
                DecodeFailure::InvalidYaml { reason, .. } => budget.text(reason)?,
                DecodeFailure::InvalidText(findings) => {
                    budget.entries(findings.len().checked_mul(16).ok_or_else(invalid)?)?;
                    for finding in findings {
                        budget.text(&finding.path)?;
                    }
                }
                _ => return Err(Error::Original(owner)),
            }
        }
        I::Preparation { schema, error, .. } => match error.as_ref() {
            E::Traversal(
                G::Unavailable { declaring, path } | G::InvalidParent { declaring, path },
            ) => {
                budget.entries(16)?;
                budget.text(declaring)?;
                budget.text(path)?;
            }
            E::Traversal(G::Cycle { path, .. }) => {
                budget.entries(path.len().saturating_add(16))?;
                for text in path {
                    budget.text(text)?;
                }
            }
            E::Traversal(G::Version {
                source,
                entry,
                expected,
                actual,
                ..
            }) => {
                budget.entries(16)?;
                for text in [source, entry, expected, actual] {
                    budget.text(text)?;
                }
            }
            E::Traversal(G::Layer {
                source,
                entry,
                input,
                error: N::Invalid(findings),
            }) => {
                for _ in findings {
                    budget.text(source)?;
                    budget.text(entry)?;
                }
                budget.schema_findings(schema, Some(input), findings)?;
            }
            E::Normalize(failure) => {
                if let N::Invalid(findings) = &failure.error {
                    budget.schema_findings(schema, failure.input.as_ref(), findings)?;
                } else {
                    supported = false;
                }
            }
            E::Model(findings) => budget.schema_findings(schema, None, findings)?,
            E::Composition(failure) => {
                if let N::Invalid(findings) = &failure.error {
                    budget.schema_findings(schema, failure.context_document.as_ref(), findings)?;
                } else {
                    supported = false;
                }
            }
            E::Dependencies(failure) => match &failure.error {
                D::Normalization(N::Invalid(findings))
                | D::Reference(R::Normalization(N::Invalid(findings))) => {
                    budget.schema_findings(schema, failure.input.as_ref(), findings)?
                }
                D::Invalid(findings) => {
                    budget.entries(findings.len().saturating_mul(16))?;
                    for finding in findings {
                        match finding {
                            F::Unknown { column, dependency } => {
                                budget.text(column)?;
                                budget.text(dependency)?;
                            }
                            F::Cycle { columns } => {
                                budget.entries(columns.len())?;
                                for text in columns {
                                    budget.text(text)?;
                                }
                            }
                        }
                    }
                }
                _ => supported = false,
            },
            _ => supported = false,
        },
        _ => supported = false,
    }
    if !supported {
        return Err(Error::Original(owner));
    }
    let encoded = crate::specification_diagnostics::inheritance_failure_ref(cause)
        .map_err(|_| Error::Original(owner))?;
    // Typed cause selection precedes projection; this protocol check validates
    // completeness, never chooses a preparation/execution route from error text.
    let value: serde_json::Value = serde_json::from_str(&encoded).map_err(|_| invalid())?;
    let outcome = &value["outcome"];
    if outcome["status"] != "invalid" {
        return Err(Error::Original(owner));
    }
    let rows = outcome["diagnostics"].as_array().ok_or_else(invalid)?;
    if rows.is_empty() {
        return Err(invalid());
    }
    complete(
        rows.iter()
            .cloned()
            .map(|row| Issue::from_diagnostic(row).map_err(|_| invalid()))
            .collect::<Result<Vec<_>, _>>()?,
        maximum,
    )
}
