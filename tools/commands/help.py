"""Print the supported local commands."""

HELP = """yamaa repository commands (run from the repository root)

  setup             Build the installed dev-profile Python extension and locked tools.
                    Needs uv, the pinned Rust toolchain and a platform C compiler.
  check             Run the five PR checks below, in the same order locally.
  check-lint        Ruff, cargo fmt and workspace Clippy (requires R as well).
  check-repository  Repository/rule validation and a strict generated docs build.
  test-rust         cargo test --locked --workspace, including both host bindings.
                    Needs the selected Python interpreter and R 4.6.1.
  test-python       Package/tools tests and standalone public-host suites outside checkout.
  test-r            Build/install a standalone R archive and require R CMD check OK.
                    Needs R 4.6.1, Rust, GNU make and the package's Suggests installed.
  check-equivalence Executor equivalence gate: build both hosts from the checkout, then
                    public benchmark conformance, installed Python suites, R CMD check.
                    Needs what test-python and test-r need. Not part of CI.
  probe-executor    Time the executor probe on a release build. Not part of CI.
  docs-test         Canonical page and benchmark dashboard tests.
  docs-generate     Generate benchmark dashboards from current authored data.
  docs-build        Strict MkDocs build (generate dashboards first).
  docs              Generate dashboards and build the strict site.

Run setup first. Native installation qualification is a separate full matrix in
.github/workflows/native.yml, on dispatch or qualification/issue-* pushes.
The tools package is installed editable so python -m tools.<module> also works
from a separate working directory. See CONTRIBUTING.md and docs/release.md.
"""

if __name__ == "__main__":
    print(HELP)
