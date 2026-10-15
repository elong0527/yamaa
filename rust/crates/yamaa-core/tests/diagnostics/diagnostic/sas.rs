//! Native codecs supply classified SAS facts at this core-owned projection boundary.
//! Core diagnostics remain independent of the adapter's C parser and filesystem.
use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode, ContextValue as V, SourceFinding},
    value::Value,
};

fn text(value: &str) -> V {
    V::Scalar(Value::Str(value.into()))
}

pub(super) fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (path, cause) in [("source.xpt", "framing"), ("source.sas7bdat", "malformed")] {
        let diagnostic = SourceFinding::Sas {
            dataset: "SRC",
            path,
            cause,
        }
        .diagnostic()
        .unwrap();
        let definition = diagnostic.definition();
        assert_eq!(
            (
                definition.phase,
                definition.condition,
                definition.requirement
            ),
            ("ingest", "source_sas_invalid", Some("REQ-1317"))
        );
        assert_eq!(diagnostic.spec_paths, ["input.SRC.path"]);
        assert_eq!(
            diagnostic.context,
            [
                ("dataset".into(), text("SRC")),
                ("path".into(), text(path)),
                ("cause".into(), text(cause)),
            ]
            .into()
        );
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
        reached.insert(diagnostic.code);
    }
    reached
}
