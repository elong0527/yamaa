# Python reference removal preparation

This is the reviewable preparation inventory for
[#1994](https://github.com/elong0527/yamaa/issues/1994). The
[transition entry gates](transition.md#entry-criteria) control every merge
that removes a reference family. A draft patch or passing local test does
not approve the transition. The prior Python source/wheel and qualified
installed R source archive remain retained outside the checkout.

## Retained ODM I/O slice

The existing readers, fixed clinical-item schema, Parquet writer and error
type move unchanged from `yamaa.odm` to `yamaa._odm`; only imports change.
The root exports `read_odm` and `write_odm_parquet`, as approved in #1973
item 1. The private package imports no specification model, expression,
binding plan, runtime context or reference evaluator. Its dependencies are
the standard library, lxml, Polars, PyArrow and Pydantic. These dependencies
remain required after semantic retirement.

Temporary legacy module shims keep the unqualified reference usable during
preparation. They delegate file I/O to the same private implementation and
add no new evaluator. The reference-removal slice removes the shims and the
rest of `yamaa.odm` together. `items.py`, `bindings.py` and `context.py`
are semantic consumers and are not moved into the retained I/O package.
#2002 must route their remaining repository-validator consumers through
installed native `check`/`domain` before that removal.

| Unique retained assertions | Qualification location |
| --- | --- |
| ODM 1.3/2.0 values, exact metadata matching, form/nested-group layouts, empty schema, absent/null/empty distinction, source order, TAR selection and bounded/untrusted input refusal | Unchanged `python/tests/odm/test_readers.py` and `test_schema.py`, importing root helpers/private schema. |
| Bounded Parquet batches, fixed logical schema/metadata, empty output, no overwrite, successful replacement, failed parse leaves old output intact, no staging leftovers | Unchanged `python/tests/odm/test_parquet.py`, importing the root writer. |
| Both retained helpers operate without semantic/reference imports | `test_helper_independence.py` uses an isolated subprocess and an import-denial hook. |
| Installed root helper exports, authored ODM 1.3/2.0 values, source ordinals, null/empty distinction, logical Parquet equality and metadata, atomic overwrite refusal | `rust/tests/installed_odm_helpers.py`, copied with authored XML fixtures outside the checkout and registered for every installed Python tuple in `supplemental.json`. |

The retained helper tests currently pass 30 tests locally, including the
independence guard. The
outside-checkout script passes against the source facade with the same
import-denial hook. Qualification of a newly built installed wheel and
source distribution remains required; local source imports are not
recorded as installed package evidence.

## Ordered removal slices

| Slice | Concrete consumer/removal scope | Required replacement or stop condition |
| --- | --- | --- |
| Public callers and docs | Root legacy exports, `yamaa_domain`, `DomainRun`, `DomainRunError`, `generate_study_document`, loader/root-discovery examples. | Port to `domain`, `check`, `define`, root ODM helpers and the private style CLI. #1992 changes only benchmark runners already protected by the public ratchet. Remove `.spec`/`.inputs`/`.input`, `save(path)` and severity expectations under the approved public API disposition. |
| Assisted qualification | `required.json`, `known-gaps.json`, `missing-routes.json`; adapter-assisted CI and `qualify_*_cohort.py` consumers. | The complete public matrix and ratchet must pass first. Preserve unique order/count/ownership/error assertions in typed native or installed public suites before removing their adapter setup. |
| Reference application | `_reference_domain.py`, `application.py`, `submission/`, reference-assisted `adapters/`, reference `io/project.py` authoring/root facade. | Every public caller and validator import is gone. Native Define uses packaged Rust schema assets. Source capture tests that still import `yamaa.io` must be ported before their family is removed. |
| Planning/runtime closure | `planning/`, `runtime/` and reference-only execution tests. | Installed public original specifications, producer graph/log/cohort tests, complete benchmark outputs and typed engine lifecycle tests preserve independently authored truth. Internal planner/model object assertions retire with their approved API. |
| Scalar/verification closure | `expressions/`, `verification/`, `regex.py`, reference comparison tools/tests. | Core typed grammar, conversion, numeric, predicate, regex, reduction/window/date contracts and installed original/public fixtures replace semantic assertions. Reference math comparisons become retained historical evidence; portable math assessments remain. |
| Schema/model/I/O closure | `specification/`, `schema/`, `models/`, residual `io/` and legacy `odm/`; matching reference-only tests. | #2002 imports only native public validation and packaged schema services. Private style field order and ODM fixed schema are independently packaged. Retained native glue has no import into the removed closure. |
| Assets and workflows | Historical resolved outputs, reviewed path-history fixtures, legacy `python.yml` jobs, dead reference tests and dependencies. | Approved historical dispositions remain in Git. Native/public workflow and structural repository validation pass; remove a dependency only after its final retained consumer disappears and regenerate `uv.lock` with packaging tooling. |

Each removal slice updates `release-api.json`, its direct tests and docs,
then runs API/dependency guards, repository validation and the full installed
matrix. A still-live import, unported assertion or failed public comparison
stops that slice. Deletion commits stay separate from the helper port and
from the R rollback rehearsal, so reviewers can retain or restore one family.

## Concrete source and assertion inventory

[python-retirement-inventory.json](python-retirement-inventory.json) freezes the
pre-removal source revision, all 99 retiring source/assets files and every named
function in the 76 retiring test files (1,192 functions before parametrization).
It records the retained source/proof families and deliberate API dispositions.
The inventory makes source-level review possible; it does not claim that a list
of proof paths establishes new installed qualification.

The exact-value arithmetic, conversion, numeric literal, reduction, predicate and
numeric-lifecycle fixtures remain in their typed Rust owners. Python component
endpoints and their JSON/envelope refusal tests disappear under #1990, while the
original-specification report fixtures remain independent authored M1 truth.
Complete report comparison retains exact scalar bits, missing/empty distinctions,
types, row/node/source/callback order, handler counts and verification ledgers.
The reviewed mutation controls refuse incomplete reports, observation changes,
wrong source truth, wrong installed owner/core/runtime and altered publication
types or saved artifacts.

Public domain/check/Define suites preserve semantic output, diagnostics, logs,
argument, ownership and held Save assertions. Python planner/model objects,
custom evaluator injection, arbitrary Save destinations, exposed input/resolved
views, legacy ODM binding objects and the old submission authoring interface
retire under the approved scope. Root discovery, root containment and fallback
path expectations retire under #1905; file-kind/symlink, snapshot, byte-bound and
declaring-file path assertions retain typed adapters and installed public owners.

The separate prerequisite commits must be composed before applying the deletion
series: the complete #2002 validator delegation, qualification's numerical CLI
ports, the public file-resources port and the reviewed M1 owner comparison. The
series retains `_odm`, the private style package, all eight approved root exports,
native facades/results/callbacks and the single distribution's extension. It
removes assisted qualification manifests and the legacy reference workflow only
after their assertions and active consumers have those successors. Historical
audits remain readable through the explicit v1 audit branch; retained authored
report truth is never relabeled as a fresh reference run.

## Retained prior-package rollback preparation

The prior Python package is tag `yamaa-py-0.3.0` at source
`845a91ff9ba4ab6599efbd79fafe1c4e1dd14255`. Its retained Linux abi3 wheel was built
from that source with Rust 1.90.0 and locked dependencies. The old release had no
package assets, and registry version 0.3.0 was unavailable. The source-built
package is distinct from the qualified installed pre-cutover R candidate at
`a1a9c2d216075f331ac108aa4d3638573f103cea`; no prior R release was published.

An explicit fresh Linux CPython 3.12.14 reinstall of the exact Python wheel
verified version 0.3.0 before extracting or reading study inputs. All 135 installed
wheel members except generated `RECORD` matched the retained archive bytes.
Unchanged committed lookup, ordered aggregate and window CSV outputs passed;
each prior result saved twice after its input/specification directory was removed.
The retained prior `uv.lock`, exact dependency pins, receipts and output bytes
are in the unpublished preparation kit. This is one Linux wheel tuple, not the
complete rollback matrix or a full producer graph Save baseline.

The R source-package rehearsal passed all three platforms with their retained
pre-cutover source archives in run 38426887552 at preparation head 560c5bd3.
Rehearsal of one canonical archive on all three platforms is separately recorded
before preparing a durable single source asset. Durable retrievable asset pins,+all required rollback tuples, B17 budgets and item 11 transition approval remain
gates. Preparation kits stay unpublished until root review.
