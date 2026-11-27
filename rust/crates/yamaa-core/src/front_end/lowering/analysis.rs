//! Lower one analysis results source (DEFINE-32) into its root,
//! [`AnalysisResults`]: a source a project file writes inline, or the root
//! of an analysis results file.
//!
//! The source lowers as a specification does, totally over what the schema
//! admits, with the construct lowering a specification's submission metadata
//! uses: a where clause, a comment and a document reference are each read
//! by the one function that reads a value-level entry's, a column's and an
//! origin's. Composition refuses what a source may not declare.

use super::{Lower, Positions};
use crate::ast::{
    AnalysisDataset, AnalysisResult, AnalysisResults, Header, Meta, Paths, ProgrammingCode,
    ResultDisplay, Segment, Span,
};
use crate::schema::Document;

/// Lower the analysis results source whose root is `root`, into an arena of
/// its own. A file's root carries the header `yamaa` declares; a source
/// written in a project file has none.
pub(crate) fn lower(
    d: &Document,
    positions: Option<&Positions>,
    root: usize,
    file: bool,
) -> AnalysisResults {
    let mut lower = Lower {
        d,
        positions,
        paths: Paths::new(),
        exprs: Vec::new(),
    };
    let meta = Meta {
        path: lower.paths.root(),
        span: lower.span(Some(root), Span::UNKNOWN),
    };
    let header = file.then(|| {
        let version = lower.required_text(meta, root, "yamaa");
        Header {
            meta: version.meta,
            version,
        }
    });
    let displays = lower.displays(root, meta);
    AnalysisResults {
        header,
        displays,
        paths: std::mem::take(&mut lower.paths),
        meta,
    }
}

impl Lower<'_> {
    /// The node of the list `owner` writes in `field`, its meta and its
    /// items, or nothing when it writes no such list.
    fn listed(&mut self, owner: usize, parent: Meta, field: &'static str) -> Vec<(usize, Meta)> {
        let Some(list) = self.field(owner, field) else {
            return Vec::new();
        };
        let list_meta = self.field_meta(parent, field, list);
        let items = self.items(list);
        items
            .iter()
            .enumerate()
            .map(|(index, &item)| {
                (
                    item,
                    self.meta(list_meta, Segment::Index(index as u32), Some(item)),
                )
            })
            .collect()
    }

    fn displays(&mut self, root: usize, parent: Meta) -> Vec<ResultDisplay> {
        self.listed(root, parent, "displays")
            .into_iter()
            .map(|(node, meta)| ResultDisplay {
                id: self.named(meta, node, "id").unwrap_or(crate::ast::Named {
                    name: String::new(),
                    meta,
                }),
                oid: self.text_field(meta, node, "oid"),
                name: self.required_text(meta, node, "name"),
                description: self.required_text(meta, node, "description"),
                documents: self.documents(node, meta),
                results: self
                    .listed(node, meta, "results")
                    .into_iter()
                    .map(|(node, meta)| self.analysis_result(node, meta))
                    .collect(),
                meta,
            })
            .collect()
    }

    fn analysis_result(&mut self, node: usize, meta: Meta) -> AnalysisResult {
        let mut datasets = Vec::new();
        if let Some(mapping) = self.field(node, "datasets") {
            let mapping_meta = self.field_meta(meta, "datasets", mapping);
            for &(key, value) in self.entries(mapping) {
                let Some(name) = self.text(key) else {
                    continue;
                };
                let meta = self.meta(mapping_meta, Segment::Key(name.into()), Some(value));
                datasets.push(AnalysisDataset {
                    dataset: self.named_at(name, meta),
                    conditions: self.where_clause(value, meta),
                    where_oid: self.text_field(meta, value, "where_oid"),
                    variables: self.names(meta, value, "variables"),
                    meta,
                });
            }
        }
        let programming_code = self.field(node, "programming_code").map(|code| {
            let meta = self.field_meta(meta, "programming_code", code);
            ProgrammingCode {
                context: self.text_field(meta, code, "context"),
                code: self.text_field(meta, code, "code"),
                documents: self.documents(code, meta),
                meta,
            }
        });
        AnalysisResult {
            id: self.named(meta, node, "id").unwrap_or(crate::ast::Named {
                name: String::new(),
                meta,
            }),
            oid: self.text_field(meta, node, "oid"),
            description: self.required_text(meta, node, "description"),
            reason: self.required_text(meta, node, "reason"),
            purpose: self.required_text(meta, node, "purpose"),
            datasets,
            join: self.comment_at(node, meta, "join"),
            documentation: self.comment_at(node, meta, "documentation"),
            programming_code,
            meta,
        }
    }
}
