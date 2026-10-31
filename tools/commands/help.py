"""Print the supported local commands."""

HELP = """yamaa repository commands (run from the repository root)

  setup             Build the installed dev-profile Python extension and locked tools.
                    Needs uv, the pinned Rust toolchain and a platform C compiler.
                    Under rustup it also adds Clippy, rustfmt and rust-src to that
                    toolchain.
  quick             check-lint, then check-repository. Runs no test suite and needs
                    no R. Before a push, run it only for a change to generated files,
                    rule cards, schemas, docs generators or Python.
  check             Run lint, repository checks and the three language suites locally.
  check-lint        Ruff, cargo fmt, Clippy over the workspace except the R binding,
                    with all features and, for the crates that have them, with no
                    default features, and over the fuzz targets. Needs Clippy and
                    rustfmt added to the pinned Rust toolchain, and a C++ compiler
                    for libFuzzer.
  check-repository  Repository/rule/notice validation and a strict generated docs build.
                    The notice check needs rust-src added to the pinned Rust toolchain.
  test-rust         cargo test --locked --workspace without the two host bindings,
                    which have no Rust tests. Needs no R.
  test-python       Package/tools tests and standalone public-host suites outside checkout.
  test-r            Clippy over the R binding, then build/install a standalone R archive,
                    run every installed R suite and R CMD check --as-cran on its
                    small smoke suite; only the notes that
                    R/yamaa/cran-comments.md explains may remain.
                    Needs R 4.6.1, Rust with Clippy, make and the package's Suggests
                    installed; with checkbashisms and aspell it also checks configure
                    and spelling.
  check-equivalence Executor equivalence gate: both hosts built from the checkout for
                    public benchmark conformance, installed Python suites, R CMD check.
                    HOSTS=python or HOSTS=r runs one host's half. Needs what
                    test-python and test-r need; the Python half needs no R. CI runs
                    each half in its corresponding host lane on qualifying PRs.
  probe-executor    Time the executor probe on a release build. Not part of CI.
  docs-test         Canonical page and benchmark dashboard tests.
  docs-generate     Generate benchmark dashboards from current authored data.
  docs-build        Strict MkDocs build (generate dashboards first).
  docs              Generate dashboards and build the strict site.

Run setup first, and again after changing rust/ or python/src/ before running
Python tests: they import the installed package. Pull request CI selects the
affected checks in four lanes, so
before a push run the tests nearest the change and, after a Rust change,
cargo fmt and Clippy on the touched crates (D148 on #3030). Reproduce a red CI
job with its own target. Native installation qualification is a separate full
matrix in .github/workflows/native.yml, on dispatch or qualification/issue-*
pushes. The tools package is installed editable so python -m tools.<module>
also works from a separate working directory. See CONTRIBUTING.md.
"""

if __name__ == "__main__":
    print(HELP)
