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
