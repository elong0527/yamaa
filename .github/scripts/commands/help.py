"""Describe supported Make targets and their explicit prerequisites."""

HELP = """yamaa repository commands (run make TARGET from the repository root)

Discovery and setup:
  help              This inventory; default for make. Needs Python 3.12+ and GNU make.
  sync-python       uv 0.11.6, Rust pinned in rust/rust-toolchain.toml; locked test env.

Repository feedback (requires sync-python; no release qualification):
  check-fast        Ruff formatting/lint and CI selectors/command controls.
  check-repository  Validator tests, repository validation, style and rules guards.
  check-blockers    Live issue blocker guard; requires GitHub API access/token.
  test-rust         Locked Rust debug, engine test-support and release-test suites.
                    Needs pinned Rust/Cargo, platform C compiler and Python 3.12+.

Installed host feedback (outside checkout; does not qualify the full matrix):
  stage-python      HOST_WORK=fresh external directory; sync-python provides codec.
  test-python       HOST_PYTHON=installed-wheel/source interpreter, HOST_WORK=staged
                    directory, OUTPUT=fresh reports, PACKAGE_FORM=wheel|source,
                    ARTIFACT_REFERENCE=installed bytes, HOST_ARTIFACT_REFERENCE=host
                    wheel, EVIDENCE=actual run URL/path; SOURCE_REVISION defaults HEAD.
                    Install yamaa, yamaa-benchmarks and locked test dependencies first.
  stage-r           R_STAGE=fresh external package directory; needs py-yaml12==0.2.0.
  stage-r-tests     HOST_WORK=fresh external directory; cataloged R supplemental suites.
  test-r            HOST_WORK=staged directory, OUTPUT=fresh reports, R_LIBS=installed
                    library, ARTIFACT_REFERENCE=source bytes, EVIDENCE=actual run.
  check-r           R_ARCHIVE=built source tarball, HOST_WORK=fresh external check dir,
                    R_LIBS=installed library/dependencies; requires R Status: OK.
                    Build/install staged source and yamaabenchmarks first; R 4.6.1,
                    pinned Rust, C compiler/GNU make (Windows: Rtools45/GNU target).
                    PUBLIC reports can be retained with YAMAA_PUBLIC_REPORT_DIR.

Documentation (DOCS_PYTHON defaults PYTHON; needs pinned docs/requirements.txt
and .github/scripts/benchmark-docs/requirements.txt installed in that interpreter):
  docs-test         Canonical site page build tests.
  docs-generate     Generate all benchmark dashboards.
  docs-build        Strict MkDocs build; generate dashboards first.
  docs              Dashboard generation followed by strict site build.
                    Deployment additionally renders slide decks with Quarto.

Independent evidence audit (PYTHON needs locked table dependencies; no engine run):
  audit-native      RUN_ID=exact Actions run, OUTPUT=fresh directory; gh API access.
  audit-replay      AUDIT_INPUT=retained snapshot, OUTPUT=fresh directory; offline.
                    Both require a passing independent --check verdict.

Release qualification: the approved 21 installed tuples, complete conformance,
shared Rust/math and independent audit run in .github/workflows/native.yml.
Use a qualification/issue-* push for complete exact-source native checks.
An ordinary dispatch omits shared Rust/math; PR feedback samples Linux hosts.
See docs/development.md and docs/release.md for current architecture/release gates.
PYTHON selects the command interpreter; CARGO_TARGET_DIR can isolate Rust builds.
"""

if __name__ == "__main__":
    print(HELP)
