//! Convert owned Rust issues to an R data frame without a host JSON decoder.
use extendr_api::prelude::*;
use yamaa_adapters::issue_rows::Issue;

pub(super) fn frame(issues: &[Issue]) -> std::result::Result<Robj, String> {
    let count = i32::try_from(issues.len()).map_err(|_| "issue row limit")?;
    let paths = List::from_values(
        issues
            .iter()
            .map(|issue| Strings::from_values(&issue.spec_paths).into_robj()),
    );
    let requirements = Strings::from_values(
        issues
            .iter()
            .map(|issue| issue.requirement.as_deref().unwrap_or(<&str>::na())),
    );
    let mut result = list!(
        phase = Strings::from_values(issues.iter().map(|issue| &issue.phase)),
        condition = Strings::from_values(issues.iter().map(|issue| &issue.condition)),
        requirement = requirements,
        spec_paths = paths,
        context = Strings::from_values(issues.iter().map(|issue| &issue.context))
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
