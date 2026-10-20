//! Original YAML rounding is one admitted scalar operation, not a formula rewrite.
use super::*;
use crate::dataset::Rounding;

pub(super) fn prepare(
    d: &Document,
    payload: usize,
    path: &str,
    _driver: Option<&str>,
    _intermediates: Option<&intermediates::Declarations>,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<Rounding, PrepareError> {
    // Schema admission checks the written source/digits fields before this
    // compiler. Keep its integer declaration, including widths beyond i64.
    for &(name, _) in mapping(d, payload)? {
        let name = text(d, name)?;
        if !["source", "digits"].contains(&name) {
            reject(extra, name, format!("{path}.{name}"));
        }
    }
    let N::Integer(digits) = &d.nodes()[field(d, payload, "digits")?] else {
        return Err(PrepareError::Internal);
    };
    let source = text(d, field(d, payload, "source")?)?;
    Rounding::new(source, path, digits).map_err(|_| PrepareError::Internal)
}

/// The source operand binds as the scope decides ([`BindContext::operand`]).
pub(super) fn bind(
    rounding: &Rounding,
    column: usize,
    cx: &mut BindContext<'_, '_>,
) -> Result<Option<Expression>, BindError> {
    cx.operand(rounding.source(), &format!("{}.source", rounding.path()))
        .map(|source| source.map(|source| wrap(rounding, column, source)))
}
fn wrap(rounding: &Rounding, column: usize, source: Expression) -> Expression {
    Expression::Round {
        source: std::boxed::Box::new(Assignment {
            column,
            expression: source,
            path: format!("{}.source", rounding.path()),
        }),
        rounding: rounding.clone(),
    }
}
