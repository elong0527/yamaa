# Assess the yamaa R package 0.5.0 on an SDTM to ADaM study

You are an AI assistant helping a statistical programmer assess the installed
yamaa R package 0.5.0 on one organizational study. Use R for installation,
specification checks, study execution, comparison, and report generation.
Assess the complete R user experience, starting from installation and ending
with reviewed yamaa specifications, a reproducible comparison, and feedback
that can safely be shared on GitHub. The statistical programmer owns the study
evidence; a second programmer or study lead reviews the derivations and the
final conclusions. If the R package cannot complete a step, record the blocker
and continue independent work through R.

The goal is to express all SDTM-to-ADaM derivation logic in idiomatic yamaa
specifications and reproduce every in-scope reference ADaM dataset. Matching
observed values alone does not establish matching derivation logic: review
every rule, including branches the study data do not exercise. Report partial
results and blockers honestly; never claim 100% equivalence by dropping a
dataset, variable, record, or unresolved rule.

Organize every generated study deliverable using the
[Pilot 7 study folder structure](https://github.com/RConsortium/submissions-pilot7-synthetic-data#folder-structure)
and its [folder manifest](https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/.github/repo-structure.json).
Use the selected `<study>/` as the workspace root throughout this recipe.
The directory tree and path rules in section 4 apply to all steps, including
installation evidence and reports.

## 1. Install and verify the yamaa R package 0.5.0

Start the job clock and effort/usage ledger before installation. Work in an
organization-approved environment with approved access to study data and any
AI service used for specification authoring. Keep study artifacts in the
organization's controlled study workspace.

The R package 0.5.0 is distributed as an asset of the
[yamaa-v0.5.0 release](https://github.com/elong0527/yamaa/releases/tag/yamaa-v0.5.0).
It is not published on CRAN. Download `yamaa_0.5.0.tar.gz` from that release
or the organization's approved mirror. This is the R source package; use an
organization-approved writable R library. It needs R 4.4 or later,
Rust/Cargo 1.82 or later, and a C toolchain, including the appropriate Rtools
and Rust target on Windows as described in the
[0.5.0 R installation guide](https://github.com/elong0527/yamaa/blob/yamaa-v0.5.0/R/yamaa/README.md#installation).
The archive bundles its Rust dependencies for offline compilation. Record
compiler detection, compilation time, permissions, and any installation
failure as part of the R package assessment.

```r
dir.create("log", showWarnings = FALSE)
install.packages("yamaa_0.5.0.tar.gz", repos = NULL, type = "source")
library(yamaa)
stopifnot(as.character(packageVersion("yamaa")) == "0.5.0")
writeLines(c(capture.output(sessionInfo()), find.package("yamaa")),
           "log/yamaa-environment-r.txt")
write.csv(installed.packages()[, c("Package", "Version", "LibPath")],
          "log/yamaa-environment-r-packages.csv", row.names = FALSE)
```

Run this installation example from the selected study root. Ensure subsequent
`Rscript --vanilla` sessions use that same library, setting
`R_LIBS_USER` through the approved environment configuration when needed.
Record the release asset/reference, installed package location, OS/architecture,
R and compiler versions, dependency versions, and manual steps. Retain the
study's `renv.lock` if it uses renv. Inspect the installed package help for
`yamaa_spec` and `yamaa_project`; perform a small synthetic R check/run/save
smoke test before opening study data. Verify that the R outputs, issue tables,
and saved artifacts can be inspected with the comparison tools.

Use the [0.5.0 rules](https://github.com/elong0527/yamaa/tree/yamaa-v0.5.0/rules),
[schemas](https://github.com/elong0527/yamaa/tree/yamaa-v0.5.0/yaml), and
[API](https://github.com/elong0527/yamaa/blob/yamaa-v0.5.0/docs/articles/api.md)
for the baseline. Review current repository best practices for improvements,
but use only syntax and behavior supported by 0.5.0 in this test. A check on
a later release or development revision is a separate, versioned experiment;
it must not replace the 0.5.0 result.

## 2. Confirm resources and freeze the test scope

Complete `log/yamaa-resource-inventory.csv` before authoring. For every item,
record its controlled location, version or snapshot reference, owner,
readability, relevance to each ADaM dataset, and status: `available`, `missing`,
`unreadable`, `version-conflict`, or `not-applicable` with a reason. Confirm
availability by opening/inspecting the resource, not by assuming a path exists.

| Resource | Confirmation required |
| --- | --- |
| SDTM inputs | All required domains, including supplemental/relationship domains; same study and data cut; row counts, columns, logical types, keys, dates, encodings, and special missing values. |
| Reference ADaM outputs | Independently produced datasets for every target; production/QC status, data cut, row/column counts, keys, and metadata. Keep these read-only. |
| Existing specifications | Dataset/variable derivations, selection rules, ordering/ties, SAP, Define-XML, controlled terminology, amendments, and study conventions; record which versions apply. |
| Existing SAS or R code | Driver programs, dataset programs, macros/functions, formats/catalogs, lookup tables, libraries, configuration, execution order, software versions, logs, and ability to rerun. Record both languages' availability, even if only one was used. |
| Other derivation inputs | Randomization, adjudication, coding dictionaries, external data, analysis cutoffs, and other non-SDTM sources actually used by the reference process. |
| Execution and measurement | Writable output space, approved readers/converters, reference runtime/license if needed, reviewer, and access to AI token/billing records and compute charges. |

Create `log/yamaa-scope.csv` with one row for every target ADaM dataset, its
reference location, expected variables/record count, comparison key and order,
dependencies, and readiness/blockers. Include ADSL and each downstream dataset;
do not assume subject identifiers alone identify BDS or occurrence records.
List helper artifacts separately from ADaM targets. Freeze the scope before
execution; retain the original and explain any approved scope revision.

The SDTM and reference ADaM snapshots must correspond to the same cut. If
conversion is needed, retain originals and the conversion code/log. Validate
record counts, logical types, labels, date/time representations, numeric
precision, text encoding, and missing-value distinctions against the originals.
Do not silently trim strings, round numbers, or collapse SAS special missing
values. Document any necessary representation mapping before comparison.

Proceed to full validation only when inputs, reference outputs, and enough
reviewed specification/code evidence are available for each target. Missing
specifications can be reconstructed from code and reviewed; missing code can
be recorded while testing a reviewed specification. If intent is ambiguous,
resources conflict, or a reference output is absent, mark the affected target
blocked or exploratory. It remains in the original scope and cannot receive
an unqualified derivation-equivalence conclusion. A reference rerun that fails
to reproduce its frozen outputs is itself a source finding.

## 3. Reconcile source evidence and map every derivation

Before translating, create `log/yamaa-derivation-traceability.csv`. Give each
rule a stable ID and record the target dataset/variable or record-selection
rule, source SDTM fields, specification section/version, code location/version,
reference evidence, proposed yamaa location, expected behavior, review status,
and test evidence. Cover all output columns and all rules controlling rows,
joins, aggregation, grouping, ordering, and deduplication.

Review at least population flags, treatment assignment, analysis dates/days,
partial-date imputation and flags, baseline selection and ties, visit windows,
unit conversions, change/percent change, analysis flags, sequence numbers,
event/censoring rules, and dictionary-dependent derivations where applicable.
Use synthetic cases with independently authored expectations for unobserved
branches and boundaries. Mark untested branches explicitly; do not infer that
their logic is verified from a complete match on the study's observed records.

Maintain `log/yamaa-source-discrepancies.csv` throughout the job:

| Field | Required content |
| --- | --- |
| Finding and impact | ID, affected rule/datasets/variables, severity, affected record/cell counts, and whether execution or sign-off is blocked. |
| Conflicting evidence | What the existing specification says, what SAS/R code does, and what the frozen SDTM/reference ADaM support; distinguish unavailable evidence from contradictory evidence. |
| Classification | Specification vs code; code vs reference output; specification vs reference output; input/data-cut/dependency conflict; conversion/comparison defect; yamaa specification defect; or suspected yamaa limitation/defect. |
| Resolution | Owner, agreed interpretation and rationale, approval, status, and recheck evidence. Preserve observations made before resolution. |

Use data lineage and, when available, a controlled rerun of the original
program to investigate conflicts. Do not automatically treat any one source
as the intended rule. The frozen ADaM is the reproduction target; the study
lead decides intended behavior when sources disagree. If approved intent
differs from frozen output, report that discrepancy and the reproduction
result separately. Never alter reference datasets, source code, or existing
specifications just to make yamaa pass. A corrected reference requires a new
approved snapshot and a separately reported comparison.

## 4. Author idiomatic specifications

Use the Pilot 7 stage folders in the selected study. Preserve its existing
files and create missing stage folders; retain an empty `.gitkeep` in any
required folder that would otherwise be empty in Git. ODM, SDTM-generation,
and TLF folders remain part of the layout even when this assessment only
derives ADaM.

```text
<study>/
  data/
    odm/                    # existing source ODM or .gitkeep
    sdtm/                   # frozen SDTM inputs / validated staged copies
    adam/
      adsl.parquet          # existing reference ADaM, read-only
      adsl-yamaa.parquet    # generated candidate; one per target dataset
  program/
    odm/                    # existing ODM programs or .gitkeep
    sdtm/                   # existing SDTM programs or .gitkeep
    adam/
      r/
        run.R
        compare.R
  spec/                     # retain existing specifications and metadata
    define/                 # existing Define-XML, when available
    yamaa/
      project.yaml
      adsl.yaml
      adae.yaml
      r/                    # optional R environment/function definitions
  tlf/                      # clinical tables, listings, figures or .gitkeep
  log/
    yamaa-run.log
    yamaa-compare.log
    yamaa-adsl-report.csv    # one engine report per generated dataset
    yamaa-engine-report.rds
    yamaa-resource-inventory.csv
    yamaa-study-report.md
    yamaa-dataset-metrics.csv
    yamaa-job-metrics.json
    yamaa-public-summary.md  # reviewed synthetic feedback drafts
```

Reference ADaM may instead be XPT, RDS, or another inventoried format; preserve
its original filename and bytes under `data/adam/`. Preserve original SAS/R
programs under `program/adam/` and original specifications under `spec/`.
Generated candidates use the `-yamaa` suffix so they cannot overwrite the
reference datasets. Scope and comparison map each reference filename to its
specific generated candidate; do not infer targets from every file in a
directory or mistake reference files for unexpected candidate outputs.
Compare the inventory of generated `*-yamaa.parquet` files with the frozen
scope and flag missing or unexpected candidates separately from references.

The manifest keeps `.parquet` and `.xpt` files directly in `data/sdtm/` or
`data/adam/`. Use CSV for per-dataset engine report files in `log/`, and RDS
for saved R issue tables and the combined engine report. Keep assessment
reports, metrics, inventories, discrepancies, logs, and public-feedback drafts
as `log/yamaa-*` files. Clinical TLF outputs belong in `tlf/`. All these
assessment files remain confidential until the public drafts are cleared.
Do not introduce top-level `adam/`, `reference/`, `reports/`, or
`public-feedback/` folders. Check the generated tree against the referenced
manifest, and use the study repository's Structure Check workflow when
available. Record the layout reference revision in the inventory.

The programmer creates the study-specific files in this recipe; the report
and ledger filenames below describe required deliverables, not built-in
yamaa features. Do not add study orchestration, converters, or reporting
utilities to the yamaa repository's `.github/scripts`.

- Create exactly one specification per output dataset under `spec/yamaa/`,
  such as `spec/yamaa/adsl.yaml`.
  Keep temporary values and ordered calculations inside it as internal
  columns, intermediates, or ordered derivations. Do not split one dataset
  into `adsl-s1.yaml`, `adsl-final.yaml`, or staged workaround datasets.
- Prefer native yamaa operations and succinct, reviewable rules. Original
  SAS/R code is evidence of intent, not a line-by-line template. Any required
  project function must be declared, versioned, reviewed, and independently
  tested through the R implementation environment; do not hide the study
  pipeline in a function or preprocessing step. Link the R implementation
  environment from `spec/yamaa/project.yaml` with
  `environments: {r: r/environment.yaml}`; declare its lock and function-file
  references in that environment file, following the 0.5.0 API.
- Put uniform derivations at column level: key echoes, literals, and common
  assignments. Keep row-phase-dependent aggregates, windows, named
  intermediate reads, or reads from differing row drivers in their templates
  when required by the 0.5.0 rules. Minimize duplicated template logic.
- Read input datasets directly. A named intermediate should add a filter,
  selection, custom key, handler, or other necessary behavior.
- Declare a column after the columns its derivation references. Keep ordered
  window chains such as `rank` -> `case` -> `previous_non_missing` in the
  same dataset specification.
- Derive subject-level variables once in ADSL and consume the derived ADSL
  downstream. Use explicit producer `schema` links and matching artifact
  paths so project execution orders dependencies and reads current results.
  Include every required producer in the selected project run scope.
- Declare every output column, its type and `label`, and the required
  `output.columns` order. From a specification in `spec/yamaa/`, use
  `output.path: ../../data/adam/<dataset>-yamaa.parquet` and
  `output.report: ../../log/yamaa-<dataset>-report.csv`. An SDTM input at
  `data/sdtm/dm.parquet` is read as `../../data/sdtm/dm.parquet`.
  Resolve each relative path from its declaring YAML file; project spec
  listings and sibling producer `schema` links remain `adsl.yaml`, etc.
- Declare meaningful verifications for keys, populations, ranges, expected
  counts, and other study invariants. Best-practice authoring must preserve
  approved study logic; propose methodological changes as separate findings.

After creating the project file described below, initialize
`project <- yamaa_project("spec/yamaa/project.yaml")` from the study root in R.
Check every specification with
`project$spec("ADSL")$check()` and inspect `project$spec("ADSL")$resolve()`;
repeat for every target. Review the 0.5.0 field order, spacing, line width,
uniform derivations, redundant intermediates, and output-column labels.
Write literals as `{literal: X}` and source reads using the documented
bare-string spelling. The R package 0.5.0 has no exported style checker;
record this authoring-tool gap and perform the style review explicitly.
Use `{literal: null}` when a handler must return a missing value; a bare
`null` handler is absent, which can leave the default error behavior in place.
Do not assume older operation names or compute functions remain supported.
Review the resolved specifications and traceability map with the second
programmer.

## 5. Run the study and handle blockers

List all target specifications in `spec/yamaa/project.yaml` under `adam.specs`.
A minimal 0.5.0 project, before adding study terminology or project functions,
is:

```yaml
schema_version: "1.0"
adam:
  specs: [adsl.yaml, adae.yaml]
```

`schema_version: "1.0"` selects the schema bundle used by package 0.5.0; it
is not the package version. In a downstream specification such as
`spec/yamaa/adae.yaml`, declare its input and output paths explicitly:

```yaml
input:
  ADSL:
    path: ../../data/adam/adsl-yamaa.parquet
    schema: adsl.yaml

output:
  path: ../../data/adam/adae-yamaa.parquet
  report: ../../log/yamaa-adae-report.csv
```

Project execution builds linked producers before consumers: typically ADSL,
followed by dependent ADaM datasets, with additional links such as ADAE to
ADTTE when required. A separately run consumer needs its predecessor's saved
artifact. Never feed reference ADaM outputs into the candidate derivation to
bypass a missing dependency; account for approved external inputs in the
inventory.

Keep `program/adam/r/run.R` within 30 lines using the installed R package.
From the study root, pass that root explicitly:
`Rscript --vanilla program/adam/r/run.R .`. Capture its stdout and stderr in
`log/yamaa-run.log`; capture comparison output in `log/yamaa-compare.log`.

```r
library(yamaa)
stopifnot(as.character(packageVersion("yamaa")) == "0.5.0")
args <- commandArgs(trailingOnly = TRUE)
stopifnot(length(args) == 1L)
root <- normalizePath(args[[1]], mustWork = TRUE)
reports <- file.path(root, "log")
dir.create(reports, showWarnings = FALSE)
tryCatch({
  project <- yamaa_project(file.path(root, "spec", "yamaa", "project.yaml"))
  checked <- project$check()
  saveRDS(checked$issues, file.path(reports, "yamaa-preflight-issues.rds"))
  if (nrow(checked$issues)) stop("Resolve preflight issues before running")
  runs <- project$run("adam")
  saveRDS(runs$issues, file.path(reports, "yamaa-run-issues.rds"))
  saveRDS(runs$report, file.path(reports, "yamaa-engine-report.rds"))
  missing_output <- vapply(runs$names, function(name) is.null(runs[[name]]$output),
                           logical(1))
  if (nrow(runs$issues) || any(missing_output)) stop("Study run incomplete")
  runs$save()
}, yamaa_error = function(error) {
  saveRDS(list(message = conditionMessage(error), issues = error$issues),
          file.path(reports, "yamaa-failure.rds"))
  stop(error)
})
```

Use `readRDS()` to inspect the saved R issue tables, engine report, and failure
details; RDS preserves list columns and diagnostic locations without another
package. Retain the condition message even when an R error has no issue rows.
Static checking does not establish successful runtime phase planning; a clean
`check()` must still be followed by the complete `run()` and saved-output
comparison. In particular, grouped row filters read completed, unqualified
row-derived columns; use supported row derivations rather than putting
qualified lookup references directly in a grouped filter.
Retain each attempt's logs, issue tables, and reports before retrying, using
attempt identifiers in the log filenames. Archive previous candidates in a
controlled snapshot that retains the same study layout; clear only the
previous generated `data/adam/<dataset>-yamaa.parquet` targets before the next
attempt so stale outputs cannot pass comparison after a failed run. Keep
current Parquet outputs directly in `data/adam/`. Time generation and
comparison separately with an external job wrapper/ledger; also record
installation, investigation, authoring, review, and rework. A successful
`runs$save()` alone is not equivalence. Assess R data-frame types, missing
values, dates/times, and error conditions. Record a value the R interface
cannot represent exactly as a blocker rather than rounding or coercing it.

If a specification is blocked, reduce it to a minimal **synthetic** single-spec
case. Record the 0.5.0 diagnostic or timeout, expected/actual behavior, and
impact. Optionally reproduce it against the latest yamaa revision in an
isolated environment, recording that revision and result separately. Keep the
dataset marked blocked in scope and metrics, along with affected consumers;
continue independent datasets. Do not commit a staged workaround as the
intended derivation. Prepare a GitHub-ready finding as described below.

## 6. Independently compare every output and rule

Implement comparison in `program/adam/r/compare.R`, using an independent R
reader and a documented comparison contract. Resolve paths from the explicit
study root, read only the reference/candidate pairs frozen in scope, and
write comparison evidence to `log/yamaa-*`. Stage approved dependencies,
for example `arrow` for Parquet, `haven` for SAS datasets/transport files and
tagged missing values, and `jsonlite` for JSON metrics, when those formats
require them. Record their versions separately from yamaa's runtime needs.
Compare the saved outputs from the current successful attempt to the frozen
reference; never generate expected results from the candidate specifications.
Require a successful attempt record with zero issues and verify that each
candidate belongs to it. Reject stale outputs and inputs or specifications
changed after that attempt began. Test the comparator with an independently
matching control and deliberate cell, key, missing-output, stale-output and
failed-attempt cases in disposable fixtures.

1. Check the complete dataset inventory and all columns, their order, logical
   types, and required metadata, including labels. Report missing/extra
   datasets and columns. Verify required metadata against original artifacts
   if a staging format cannot retain it.
   Check declared YAML labels and serialized Parquet labels independently.
   The released R 0.5.0 save path can omit `yamaa:label` field metadata even
   when every declaration matches. Reproduce this on invented data, retain
   the strict failure, and report any explicit publication workaround
   separately; never copy reference labels into candidates to conceal it.
2. Align records by the reviewed complete key, checking key uniqueness and
   missing/extra records first. Use independently defined occurrence identity
   for legitimate duplicates; do not sort on all compared values, pair by row
   position, or deduplicate to conceal a mismatch. Test required output order
   separately from key-based alignment.
3. Compare every reference cell, including identifiers, flags, dates/times,
   strings, integers, and missing values. Equality uses the agreed logical
   representation; record conversion mappings and preserve meaningful
   missing-value distinctions. Do not coerce types or trim/round values to
   manufacture equality.
4. Count exact numeric matches separately from differences within an absolute
   tolerance of `1e-10` (relative tolerance zero). Only apply this diagnostic
   to finite floating-point values; integers, dates, flags, and text require
   exact equality. Report maximum absolute difference and counts outside
   tolerance. A tolerance-only match is not an exact cell match.
   Preserve the source's arithmetic association: `(value - base) / base *
   100` and `100 * (value - base) / base` can differ in floating point.
   Review an association correction as a source discrepancy; do not round
   or make value-specific changes to force matching.
5. Retain restricted mismatch details: dataset, record key, variable,
   reference/generated values, difference, rule ID, classification, owner,
   and resolution status. Trace each discrepancy back to the source-evidence
   review rather than assuming it is an engine bug.
6. Check the traceability map and independently expected synthetic cases for
   every rule/branch, including those unobserved in the study. Record rule
   coverage, missing evidence, and reviewer sign-off alongside data matching.

`program/adam/r/compare.R` must report matched/total columns and cells for each
dataset, structural/metadata differences, and scope coverage. It exits nonzero
on any mismatch, missing or unassessable target, or validation issue. If the
study lead accepts numeric tolerance differences for a separate practical
assessment, retain the strict result and label the acceptance explicitly;
do not relabel it as 100% exact matching.

## 7. Produce the confidential study report

Deliver `log/yamaa-study-report.md` plus machine-readable
`log/yamaa-dataset-metrics.csv`, `log/yamaa-job-metrics.json`, and the ledgers
below. Populate actual values with units and evidence; use `not-measured` or
`unavailable` with a reason when instrumentation or billing access is absent.
Never invent measurements or substitute zero for unknown usage.

| Overall metric | Definition |
| --- | --- |
| Datasets fully matching | Count / all frozen in-scope target ADaM datasets. A full match requires complete structure, metadata, record identity/order, every exact cell, and zero validation issues. Also show matched, mismatched, blocked, and not-compared counts, with mutually exclusive statuses. |
| Cells fully matching | Sum of exact matching reference cells / sum of reference rows x reference columns over the entire frozen scope. Missing rows/columns do not match; blocked or unassessed cells remain in the denominator when known, with their count shown separately. If any reference size is unknown, report the known subtotal and mark the overall rate incomplete. |
| Additional data differences | Missing/extra datasets, columns, and records; extra cells separately; metadata/type/order differences; tolerance-only and outside-tolerance numeric counts. Matching all reference cells cannot establish a full match when extras exist. Empty datasets have zero cells; report their structural result and cell percentage as not applicable. |
| Derivation coverage | Reviewed and independently tested rules/branches / all inventoried rules/branches, including unobserved paths; unresolved or unassessable rules and source contradictions. |
| Time | Start/end with time zone, end-to-end wall time from installation to final report, phase elapsed times, engine execution and comparison durations, active programmer/reviewer hours, attempts, retries, and rework. Concurrent phase durations need not sum to wall time. |
| Tokens | Actual provider/model usage across every job-related authoring, debugging, review, and retry call: input, cached-input subset, output, and other separately billed categories where reported. Avoid double-counting cached or reasoning subsets. Include failed calls when charged, model/version, and unavailable counters. |
| Money | Total job cost in a stated currency, with AI/API, compute/runtime, and other charged tools itemized; billed actuals versus estimates, rate source/date, and inclusion of tax, discounts, subscriptions, or credits. Report labor separately using approved rates if available. Unknown costs make the total incomplete. |

Do not average dataset percentages to obtain the cell rate. Show a per-dataset
table containing status, reference/generated rows and columns, matched/total
columns, exact matched/total reference cells, extra records/cells, metadata
differences, tolerance-only matches, validation issues, and elapsed time.
State whether column matches refer to structure or complete column contents.
Keep logical derivation coverage separate from the dataset reproduction count.

Maintain `log/yamaa-usage-ledger.csv` with attempt/phase, provider/model, usage
categories, units, rate, amount, and billing evidence reference. Compute an
estimated charge as the sum of each disjoint billed quantity times its
applicable unit price; label estimates and reconcile them with invoices when
available. Include setup and unsuccessful attempts in job totals, even when
only the final execution contributes data-match metrics. The yamaa engine
report does not supply AI token or monetary totals; collect these from the
authoring platform/provider and execution environment.

The report must also include resource readiness, frozen scope and versions,
methods/normalizations, per-dataset results, source discrepancies and their
resolutions, remaining blockers, reviewed rule coverage, improvement findings,
and a reproducible run/comparison procedure. Retain internal evidence with
access controls. Conclude `complete`, `partial`, or `blocked`, and state
exactly which equivalence criteria are met. Full success requires every
target to match, zero validation issues, complete reviewed derivation evidence
and tests, and no unresolved source conflict affecting the conclusion.

## 8. Prepare GitHub-ready yamaa improvement feedback

Create a separate `log/yamaa-public-summary.md` and
`log/yamaa-public-<finding-id>.md` for each actionable finding. Cover user
interface (installation, authoring, diagnostics, review,
comparison), specification schema (clarity, expressiveness, repetition,
traceability), engine (correctness, supported operations, determinism,
performance), and documentation/tooling as supported by observed evidence.
Use R installation steps, `yamaa_spec()`/`yamaa_project()` calls, package help,
R data frames, and `yamaa_error` conditions as the evidence for the user
interface assessment. Identify whether a finding belongs to the R package,
the specification schema, or the shared engine reached through R.
Distinguish study/source defects from product defects, missing capabilities,
and usability suggestions. If an area has no finding, say so.

Each finding must stand on its own for a GitHub maintainer:

- Concise title, category, tested R package and R versions, and relevant
  public platform details; identify a separately tested development revision
  if applicable.
- User task and observed problem, expected behavior, impact, priority, and a
  measurable acceptance criterion.
- Minimal synthetic inputs/specification/R code, `Rscript` reproduction
  commands, and sanitized actual/expected output or diagnostic. Build
  synthetic values independently; anonymizing rows from the study is not a
  public fixture.
- Suggested improvement and any workaround, clearly distinguished from a
  confirmed root cause. Include only performance measurements reproduced on
  the synthetic case and authorized aggregate metrics.

Exclude organization/sponsor/study/site/subject identifiers, proprietary
derivation rules or code, original datasets or specifications, internal paths,
links, credentials, service details, and unreviewed logs/screenshots. Inspect
attachments and metadata as well as prose. Keep the mapping from public
findings to confidential evidence only in the controlled study workspace.

Have the study owner review the public bundle for disclosure before sharing.
The bundle should be ready to paste into GitHub without access to internal
resources; preparing it does not authorize posting study material. After
organizational clearance, product blockers may be filed with the
`pilot-dry-run` label, subject to repository permissions. Keep confidential
study reporting and public product feedback as separate deliverables.
