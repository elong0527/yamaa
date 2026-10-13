# Shared-engine compatibility

[Maintainer decisions](../rust/planning/maintainer-decisions.md) define the
first-release scope. Domain/check/Define are owned by the shared Rust engine;
Python and R convert host arguments and copied result views. The independently
reviewed rules and fixtures define semantics. A disagreement requires a repair
or explicit reviewed disposition, never regenerated expected output.

| Retained surface | Required behavior |
| --- | --- |
| Python `domain` / R `yamaa_domain` | Original-file preparation, one ordered workflow, declared host functions/terminology, copied output/issues/logs and explicit held-byte Save. |
| Python `check` / R `yamaa_check` | Preparation/static diagnostic issues only; no study data or project-code execution. |
| Python `define` / R `yamaa_define` | Fixed-time environment submission rendering, complete validation before Save, ordered Dataset-JSON/XML publication. |
| Package version | Python `__version__` and R `packageVersion("yamaa")` identify the installed distribution. Core metadata stays private qualification evidence. |
| Python `read_odm` / `write_odm_parquet` | Fixed-schema ODM XML/TAR I/O and bounded atomic Parquet writing, independent of reference semantics. |
| Style CLI | `python -m yamaa.style`; private implementation with packaged field order and shared admission/edit proof. |

The approved result/error contracts retain owned views, full-range signed integer
values, lossless NUL text, ordinary temporal host types, interruptions/reentrancy,
verification/warning logs and repeated use. Semantic fields, identifier names,
row/column/node order, missingness, exact scalar bits, diagnostics, handler and
callback counts/order and resource capture observations remain qualified.
CSV compares exact bytes and Parquet compares logical fields/types/rows/nulls and
values. Reporting precision cannot hide changes to source or prepublication tables.

Legacy Python `yamaa_domain`/`DomainRun`, exposed planner/evaluator/model/schema/IO
and resolved/provenance APIs, and the old submission generator retire at qualified
cutover. The old R `cdiscbuilder` semantic/SQL evaluator, clinical helpers,
submission extensions and `parse_odm_to_long_df` retire under the approved scope.
They are not separate APIs translated into the current package. See the
[Python migration guide](README.md#migration-from-the-reference-api) and
[R assertion/removal inventory](../rust/planning/r-package-cutover.md).

There is one locked release per host and no public backend selector or fallback.
Project functions use ordinary packages in the declared uv/renv environment;
activation is checked freshly before every build. Resource snapshots retain bytes
and compare them directly. Runtime artifacts are pinned by
`runtime.artifact.reference`, with no content digest identity.

Shared numeric operations use the approved
[PortableLibmV1 policy](../rust/planning/1740-math-decision.md). Historical
EXP/LN/POWER bits and dependent predicates/conversions/output can differ; the
release does not promise historical bit compatibility or universal accuracy.

The complete public ratchet and installed matrix establish active language/API
coverage. Typed core grammar replay separately preserves exact syntax shapes,
identifier sets and closed vocabulary. Complete M1 public report comparisons use
unchanged committed authored truth, retain actual installed owners and saved
bytes, and import no reference evaluator. Component evidence does not substitute
for a fresh installed tuple. Reference source artifacts remain until the
[transition gates](../rust/planning/transition.md) pass and the removal sequence
is approved; preparation alone supplies no release approval.
