//! Analysis results metadata in the Define-XML document (DEFINE-31): the
//! `arm:AnalysisResultDisplays` element, and the where clause and the join
//! comment each analysis result adds to the document's definitions.
//!
//! Each shared construct is written by the function that writes it for a
//! specification's metadata: a where clause's range checks, a comment's
//! description and document references, and translated text.
use super::{described, push_shared, range_check, where_oid, Element};
use yamaa_core::submission_composition::{AnalysisDataset, AnalysisDisplay, AnalysisResult, Study};

/// The ARM v1.0 namespace, which `ODM` declares as `arm` when the document
/// carries a result display (DEFINE-11).
pub(super) const NAMESPACE: &str = "http://www.cdisc.org/ns/arm/v1.0";

/// Every result of the study, with its display, in display and result order.
fn results(study: &Study) -> impl Iterator<Item = (&AnalysisDisplay, &AnalysisResult)> {
    study.analysis_displays().iter().flat_map(|display| {
        display
            .results()
            .iter()
            .map(move |result| (display, result))
    })
}

/// The identity that closes a result's where-clause and join-comment
/// identifiers: `<display.id>.<result.id>`, whatever OID the result declares
/// (DEFINE-9).
fn identity(display: &AnalysisDisplay, result: &AnalysisResult) -> String {
    format!("{}.{}", display.id(), result.id())
}

/// The OID of the where clause of a result's analysis dataset: its received
/// `where_oid`, otherwise `WCA.<display.id>.<result.id>.<dataset.id>`
/// (DEFINE-9).
fn clause_oid(
    display: &AnalysisDisplay,
    result: &AnalysisResult,
    dataset: &AnalysisDataset,
    id: &str,
) -> String {
    where_oid(
        dataset.where_oid(),
        "WCA",
        &format!("{}.{id}", identity(display, result)),
    )
}

/// Append one where-clause definition per analysis dataset that declares a
/// clause, in display, result and dataset order (DEFINE-11); one that
/// declares a received OID an earlier where clause wrote with the same range
/// checks shares it (DEFINE-10).
pub(super) fn push_where_clauses(
    metadata: &mut Element,
    study: &Study,
    written: &mut Vec<(String, bool)>,
) {
    for (display, result) in results(study) {
        for dataset in result.datasets() {
            let Some(terms) = dataset.clause() else {
                continue;
            };
            let composed = &study.datasets()[dataset.dataset()];
            let path = format!("{}.datasets.{}.where", result.path(), composed.id());
            let mut clause = Element::new("def:WhereClauseDef", &path)
                .attr("OID", clause_oid(display, result, dataset, composed.id()));
            for term in terms {
                clause.push(range_check(&path, composed, term.term(), term.typed()));
            }
            push_shared(metadata, written, clause, dataset.where_oid().is_some());
        }
    }
}

/// Append one comment definition per result that says how its datasets are
/// joined, in display and result order (DEFINE-11).
pub(super) fn push_join_comments(metadata: &mut Element, study: &Study, language: Option<&str>) {
    for (display, result) in results(study) {
        let Some(join) = result.join() else {
            continue;
        };
        let mut node = Element::new("def:CommentDef", &format!("{}.join", result.path()))
            .attr("OID", format!("COMA.{}", identity(display, result)));
        described(&mut node, &join.text.text, &join.documents, language);
        metadata.push(node);
    }
}

/// Append `arm:AnalysisResultDisplays`, the last child of `MetaDataVersion`,
/// when the study declares a result display (DEFINE-31). Children and
/// attributes follow the order the ARM v1.0 schema gives them.
pub(super) fn push_displays(metadata: &mut Element, study: &Study, language: Option<&str>) {
    let displays = study.analysis_displays();
    if displays.is_empty() {
        return;
    }
    let mut root = Element::new("arm:AnalysisResultDisplays", "analysis_results");
    for display in displays {
        let path = display.path();
        let mut node = Element::new("arm:ResultDisplay", path)
            .attr("OID", display.oid())
            .attr("Name", display.name());
        node.translated_at(
            "Description",
            display.description(),
            language,
            Some(&format!("{path}.description")),
        );
        node.refs(display.documents());
        for result in display.results() {
            node.push(analysis_result(study, display, result, language));
        }
        root.push(node);
    }
    metadata.push(root);
}

/// One `arm:AnalysisResult`: its identity, reason and purpose, then its
/// description, the datasets it reads, its documentation and its code.
fn analysis_result(
    study: &Study,
    display: &AnalysisDisplay,
    result: &AnalysisResult,
    language: Option<&str>,
) -> Element {
    let path = result.path();
    let identity = identity(display, result);
    let parameter = result
        .parameter()
        .map(|dataset| format!("IT.{}.PARAMCD", study.datasets()[dataset].id()));
    let mut node = Element::new("arm:AnalysisResult", path)
        .attr("OID", result.oid())
        .optional("ParameterOID", parameter)
        .attr("AnalysisReason", result.reason())
        .attr("AnalysisPurpose", result.purpose());
    node.translated_at(
        "Description",
        result.description(),
        language,
        Some(&format!("{path}.description")),
    );
    let datasets_path = format!("{path}.datasets");
    let mut datasets = Element::new("arm:AnalysisDatasets", &datasets_path).optional(
        "def:CommentOID",
        result.join().map(|_| format!("COMA.{identity}")),
    );
    for dataset in result.datasets() {
        let id = study.datasets()[dataset.dataset()].id();
        let at = format!("{datasets_path}.{id}");
        let mut read =
            Element::new("arm:AnalysisDataset", &at).attr("ItemGroupOID", format!("IG.{id}"));
        if dataset.clause().is_some() {
            read.push(
                Element::new("def:WhereClauseRef", &at)
                    .attr("WhereClauseOID", clause_oid(display, result, dataset, id)),
            );
        }
        for variable in dataset.variables() {
            read.push(
                Element::new("arm:AnalysisVariable", &at)
                    .attr("ItemOID", format!("IT.{id}.{variable}")),
            );
        }
        datasets.push(read);
    }
    node.push(datasets);
    if let Some(documentation) = result.documentation() {
        let mut text = Element::new("arm:Documentation", &format!("{path}.documentation"));
        described(
            &mut text,
            &documentation.text.text,
            &documentation.documents,
            language,
        );
        node.push(text);
    }
    if let Some(code) = result.programming_code() {
        let at = format!("{path}.programming_code");
        let mut program = Element::new("arm:ProgrammingCode", &at).optional(
            "Context",
            code.context.as_ref().map(|context| &context.text),
        );
        if let Some(written) = &code.code {
            let mut text = Element::new("arm:Code", &format!("{at}.code"));
            text.text = Some(written.text.clone());
            program.push(text);
        }
        program.refs(&code.documents);
        node.push(program);
    }
    node
}
