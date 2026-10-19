"""Print the supported local commands."""

HELP = """yamaa repository commands (run from the repository root)

  setup             Build the installed dev-profile Python extension and locked tools.
                    Needs uv, the pinned Rust toolchain and a platform C compiler.
                    Under rustup it also adds Clippy and rustfmt to that toolchain.
  quick             Before a push: check-lint without the R binding, then
                    check-repository. Runs no test suite and needs no R.
  check             Run the five PR checks below, in the same order locally.
  check-lint        Ruff, cargo fmt, workspace Clippy (requires R as well) and Clippy
                    over the fuzz targets. Needs Clippy and rustfmt added to the
                    pinned Rust toolchain, and a C++ compiler for libFuzzer.
  check-repository  Repository/rule/notice validation and a strict generated docs build.
  test-rust         cargo test --locked --workspace, including both host bindings.
                    Needs the selected Python interpreter and R 4.6.1.
  test-python       Package/tools tests and standalone public-host suites outside checkout.
  test-r            Build/install a standalone R archive and run R CMD check --as-cran;
                    only the notes that R/yamaa/cran-comments.md explains may remain.
                    Needs R 4.6.1, Rust, make and the package's Suggests installed;
                    with checkbashisms and aspell it also checks configure and spelling.
  check-equivalence Executor equivalence gate: both executors compared on the Rust suites,
                    then both hosts built from the checkout for public benchmark
                    conformance, installed Python suites, R CMD check.
                    Needs what test-python and test-r need. Not part of CI.
  probe-executor    Time the executor probe on a release build; --executor resolved
                    times the executor over the resolved plan. Not part of CI.
  docs-test         Canonical page and benchmark dashboard tests.
  docs-generate     Generate benchmark dashboards from current authored data.
  docs-build        Strict MkDocs build (generate dashboards first).
  docs              Generate dashboards and build the strict site.

Run setup first, and again after changing rust/ or python/src/ before running
Python tests: they import the installed package. Pull request CI runs check; run
quick and the tests nearest the change, and reproduce a red CI job with its
own target. Native installation qualification is a separate full matrix in
.github/workflows/native.yml, on dispatch or qualification/issue-* pushes.
The tools package is installed editable so python -m tools.<module> also works
from a separate working directory. See CONTRIBUTING.md and docs/release.md.
"""

if __name__ == "__main__":
    print(HELP)
