"""Print the supported local commands."""

HELP = """yamaa repository commands (run from the repository root)

  setup             Build the installed dev-profile Python extension and locked tools.
                    Needs uv, the pinned Rust toolchain and a platform C compiler.
                    Under rustup it also adds Clippy and rustfmt to that toolchain.
  quick             Before a push: check-lint, then check-repository. Runs no test
                    suite and needs no R.
  check             Run the five PR checks below, in the same order locally.
  check-lint        Ruff, cargo fmt, Clippy over the workspace except the R binding
                    and over the fuzz targets. Needs Clippy and rustfmt added to the
                    pinned Rust toolchain, and a C++ compiler for libFuzzer.
  check-repository  Repository/rule/notice validation and a strict generated docs build.
  test-rust         cargo test --locked --workspace without the two host bindings,
                    which have no Rust tests. Needs no R.
  test-python       Package/tools tests and standalone public-host suites outside checkout.
  test-r            Clippy over the R binding, then build/install a standalone R archive
                    and run R CMD check --as-cran; only the notes that
                    R/yamaa/cran-comments.md explains may remain.
                    Needs R 4.6.1, Rust with Clippy, make and the package's Suggests
                    installed; with checkbashisms and aspell it also checks configure
                    and spelling.
  check-equivalence Executor equivalence gate: both hosts built from the checkout for
                    public benchmark conformance, installed Python suites, R CMD check.
                    Needs what test-python and test-r need. CI runs it on qualifying PRs.
  check-conversion  1.0 conversion gate: every benchmark converted to 1.0 and run on a
                    Python host built from the checkout, Define benchmarks included;
                    HOSTS=python,r adds R. Needs what test-python needs, and test-r
                    with R. CI runs it on qualifying PRs and weekly on both hosts.
  probe-executor    Time the executor probe on a release build. Not part of CI.
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
