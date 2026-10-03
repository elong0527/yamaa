# R package baseline

Run the complete testthat suite from the checkout so the shared grammar,
submission schemas and benchmark inputs remain available:

```sh
Rscript -e 'install.packages(c("dplyr", "tidyr", "purrr", "stringr", "xml2", "yaml", "yaml12", "rlang", "sqldf", "lubridate", "testthat", "pkgload"), repos="https://cloud.r-project.org")'
Rscript R/cdiscbuilder/inst/conformance/package_tests.R
```

The dependency installation is separate from the test command. The runner
reports missing dependencies, enables sqldf's R helper engine for headless use,
prints expectation/failure/error/skip counts and session information, and exits
nonzero on test failures. `yaml12` needs a Rust compiler when installed from
source; CRAN binary availability depends on the platform.

The CI workflow uses R 4.6.1 on Ubuntu 24.04, pins the action revisions and the
Rust compiler used by source dependencies, and retains the test/session log.
R dependencies are resolved from the configured CRAN-compatible repository;
their exact installed versions are recorded in each run, not locked transitively.
This establishes an executable baseline, not a bit-reproducible dependency build.

This suite covers the existing legacy R APIs, helpers, grammar and submission
checks. It does not establish current-schema R benchmark execution or Rust
backend coverage. The existing standalone grammar workflow remains separate.

The initial baseline uncovered two runtime defects: `join_visit` selected a
right-hand join key already removed by dplyr, and dose-date ordering parsed
seconds through a minute-only format. Existing tests reproduce both defects.
Preparation also repairs stale fixture assertions and an XML test generator
with too few format arguments. Committed benchmark data is unchanged.
