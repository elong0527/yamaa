//! Convert owned Rust diagnostics to an R data frame without a host JSON decoder.
use extendr_api::prelude::*;
use yamaa::Diagnostic;

pub(super) fn frame(diagnostics: &[Diagnostic]) -> std::result::Result<Robj, String> {
    let count = i32::try_from(diagnostics.len()).map_err(|_| "issue row limit")?;
    let paths = List::from_values(
        diagnostics
            .iter()
            .map(|diagnostic| Strings::from_values(&diagnostic.spec_path).into_robj()),
    );
    let requirements = Strings::from_values(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.requirement.as_deref().unwrap_or(<&str>::na())),
    );
    let mut result = list!(
        phase = Strings::from_values(diagnostics.iter().map(|diagnostic| &diagnostic.phase)),
        condition = Strings::from_values(diagnostics.iter().map(|diagnostic| &diagnostic.code)),
        requirement = requirements,
        spec_paths = paths,
        context = Strings::from_values(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.context.to_json())
        )
    )
    .into_robj();
    result
        .set_class(["data.frame"])
        .map_err(|_| "issue data-frame class")?;
    let row_names = if count == 0 {
        Integers::new(0)
    } else {
        Integers::from_values([Rint::na(), Rint::from(-count)])
    };
    result
        .set_attrib("row.names", row_names)
        .map_err(|_| "issue data-frame row names")?;
    Ok(result)
}
