//! Native codecs supply classified SAS facts at this core-owned projection boundary.
//! Core diagnostics remain independent of the adapter's C parser and filesystem.
use super::rendered;
use std::collections::BTreeSet;
use yamaa_core::diagnostic::{ConditionCode, SourceFinding};

use super::text;

pub(crate) fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    // Only `deleted_records` carries a hint: its remedy is outside yamaa, by
    // rewriting the dataset in SAS.
    for (path, cause, hint) in [
        ("source.xpt", "framing", None),
        ("source.sas7bdat", "malformed", None),
        (
            "source.sas7bdat",
            "deleted_records",
            Some(
                "rewrite the SAS dataset first, for example with PROC COPY or a DATA step, \
                 to remove the records deleted in place",
            ),
        ),
    ] {
        let diagnostic = SourceFinding::Sas {
            dataset: "SRC",
            path,
            cause,
        }
        .diagnostic();
        let definition = diagnostic.definition();
        assert_eq!(
            (definition.phase, definition.condition, definition.rule),
            ("ingest", "source_sas_invalid", Some("SAS-7"))
        );
        assert_eq!(diagnostic.spec_paths, ["input.SRC.path"]);
        let mut context = std::collections::BTreeMap::from([
            ("dataset".into(), text("SRC")),
            ("path".into(), text(path)),
            ("cause".into(), text(cause)),
        ]);
        if let Some(hint) = hint {
            context.insert("hint".into(), text(hint));
        }
        assert_eq!(diagnostic.context, context);
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
        reached.insert(rendered(&diagnostic));
    }
    reached
}
