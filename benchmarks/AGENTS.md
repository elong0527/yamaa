# Derivation benchmarks

Each subdirectory is one benchmark: one or more alternative specifications, their
shared input data, the exact output an implementation must reproduce, and a
README describing what the benchmark derives.

## Required reading

Read `../rules/README.md` and `../yaml/README.md` first; they govern the
schema and rules this directory exercises. Then read `README.md` here for
the lifecycle states; the case directories themselves are the benchmark
index. The open design
gaps and the schema work they justify are tracked in this repository's issue
tracker, one work item per root cause.

## Benchmark layout

    <standard>-<domain>-<subject>/
        benchmark.yaml             # lifecycle, standard, domain and entries
        README.md                  # the data contract
        spec.yaml                  # one entry specification
        spec_<name>.yaml ...       # alternatives, parents or producers
        project.yaml               # functions, locks, terminology, sections
        input/                     # independently authored source files
        expected/                  # exact output, logs, documents or error
        functions/<name>.yaml      # when a spec calls a project function
        r/renv.lock                # the R lock the project names

Every case has one `benchmark.yaml`; README prose supplies no configuration.
Start a case with:

```yaml
lifecycle: draft
standard: SDTM
domain: DM
```

A case with one `spec*.yaml` runs that file in both hosts. When present, the
root `project.yaml` is the project every call passes, in both hosts. Its
`functions` list names the project's function files; its `environments` map
names each host's packaging lock, and each host reads only its own entry; its
`codelists`, `study` and `sdtm`/`adam`/`send` sections carry the study
metadata. A section needs only `specs` to run; `study`, `standards` and
`define` are required only by Define-XML. No other file is selected
implicitly.

A case with several specifications declares the public calls in order. Both
hosts must be present; each call names its specification and may override its
project in the `environment` field, including an explicit `null` to call
without one. Put the
primary entry last in both host lists; the dashboard selects the final Python
call as the page's entry specification:

```yaml
entries:
  python:
    - specification: spec_dm.yaml
      environment: null
    - specification: spec_suppdm.yaml
      environment: null
  r:
    - specification: spec_dm.yaml
      environment: null
    - specification: spec_suppdm.yaml
      environment: null
```

A case whose project composes a submission section declares that project
submission in `define`, so the fixed-time Define suites and `tools.benchmark`
run it over the case's published tables and compare `expected/` byte for byte.
Its `environment` names the project file, as an entry's does, and its
`standard` the section the submission composes:

```yaml
define:
  environment: project.yaml
  standard: sdtm
  created: '2026-09-11T00:00:00'
```

`expected/` then holds the Define-XML document and each transport file the
section's `datasets` requests (`.xpt` for `xpt`, `.json` for `json`) beside
the published tables, or, for a negative case, `expected/error.yaml` with the
submission's first issue; its domain build must still succeed. `.xpt` goldens
are written by `python -m tools.conformance.xpt_goldens` from the
hand-authored tables there.

`sdtm-dm-race-ethnicity` is the worked producer/consumer example. Each producer
commits its own expected artifact beside the entry's. A `schema` link only types
the stored artifact; a consumer never runs its producer. List each producer
before its consumers: the runners build and save every entry in order, so a
later entry reads the artifact an earlier entry saved. Multi-level inheritance
and alternative specifications also declare their entries; source files are
not guessed from a parent graph. The metadata is the shared owner for native
conformance and the dashboard's generated Python and R usage snippets. Do not
add per-case runner scripts. Approved exclusions and metamorphic differences
also belong to their case's `conformance` metadata, with their reviewed reason;
they have no separate registry. A permanent difference states its by-design
reason, and a temporary difference must be pruned when it becomes stale.

Inspect the complete plan from the repository root:

```sh
python -m tools.conformance.public_conformance plan \
  --fixtures benchmarks --output /tmp/yamaa-benchmark-plan.json
```

Function implementations are ordinary installed packages under
`rust/tests/project-programs/`. Qualification stages studies outside the
checkout, copies the existing Python packaging lock and retains the R
environment's hash-free lock. The subdirectories organize fixtures; they do not
select an implicit project root.

The `sdtm-*` and `adam-*` evaluation prompts and independent solutions live
beside the Harbor harness in `../evaluations/harbor/`. Its prompt recipe is
`../evaluations/harbor/prompts/recipe.md`.

## Input staging

A benchmark's inputs come from the true upstream stage of the pipeline,
never from a halfway shape:

- `sdtm-*` benchmarks map collected data, so their inputs are ODM
  exports (or the collected form the EDC produces). A benchmark whose
  pattern is a derivation rather than a mapping (age from a birth
  date, a coding dictionary applied to a term) takes whatever upstream
  input feeds it; forcing ODM there only bloats the fixture.
- `adam-*` benchmarks derive from SDTM domains, plus ADSL for
  subject-level inputs other ADaM datasets need. Pre-ADaM `*_raw`
  inputs are a controlled isolation, not a defect: keep one when it
  isolates the pattern, and re-stage to SDTM when the input shape
  starts hiding the derivation.
- `schema-*` benchmarks demonstrate the specification language
  itself; their inputs are whatever smallest fixture exercises the
  construct.
- `negative-*` benchmarks pin a failure; their inputs are whatever
  smallest fixture triggers it.

One derivation idea per benchmark still holds: staging the input at
the true upstream stage must not smuggle in extra derivation steps.
When the honest input would add a second pattern (deriving the study
day before windowing on it), split the pipeline across benchmarks and
let each pin its own pattern.

## Fixture size

Fixtures are minimal sufficient for a human to digest: aim for 20 or
fewer rows per input file, with every row earning its place -- one
edge case, one branch, one tie-break each. ODM inputs are judged by
subjects represented rather than rows; two or three subjects carry
the same spirit. A golden past about 40 rows usually means the
fixture is too big, not the derivation too rich. More rows need a
one-line justification in the work-item issue. Existing oversized
fixtures shrink opportunistically, when their benchmark is touched
for another reason.

## A benchmark that needs project code

A `function` call names a logical function with arguments only. A benchmark
passes an explicit project file whose `functions` list names one function file
per function: a `type: function` file with the function's parameters and
tests, and, under `implementations`, the callable that implements it in each
host language. The project's `environments` map names each host's packaging
lock, and the project's paths, its function files and locks among them,
resolve from `project.yaml`. One function file serves both hosts, so the
independently written Python and R implementations run the same tests and
share the unchanged specification, project and expected outputs:

```yaml
yamaa: "1.0"
type: project
functions:
- functions/bmi.yaml
environments:
  r:
    lock: r/renv.lock
adam:
  specs:
  - spec.yaml
```

Function implementations are ordinary installed packages. The benchmark program
packages live in `rust/tests/project-programs/`; no vendored runtime artifact,
contract fingerprint or activation cache is used. The original function
studies are copied outside the checkout by `tools/staging/stage_project_inputs.py`.
It copies the sole existing `python/uv.lock` to the Python lock a project
names; committed R locks contain versions without hashes. The subdirectories
are fixture organization, not implicit project-root selection.

An expected failure before a dataset is completed replaces the CSV with
`expected/error.yaml`, unless the intended artifact is useful as a forward
contract. A failure over a completed dataset carries both the expected CSV and
`expected/error.yaml`.

A multi-level benchmark keeps each inheritance level beside the entry as
`spec_<level>.yaml` and carries no top-level `spec.yaml`. The level no other
file names as its `parent` is the entry: it declares the complete `output`, and
the chain resolves to `expected/spec_resolved.yaml`, which the dashboard shows
in its specification dropdown (the default view) rather than as an expected
dataset. Reserve this layout for benchmarks whose point is the layering itself;
a single specification with shared parents keeps `spec.yaml`.

Name the directory for what it derives, not for the construct it uses:
`sdtm-vs-study-day`, not `sdtm-vs-mapping-from`. The exception is a
benchmark whose subject is the specification language itself: name it
`schema-*` (`schema-inheritance`), and the gallery lists it under its own
Specification category rather than a data domain.

## The README describes data, not the specification

A reader of a benchmark README wants to know what the output means. The
target audience is an average statistician or statistical programmer with a
general understanding of SDTM and ADaM but limited engineering, ODM XML, or
tooling detail: expand abbreviations on first use and state effects in
study-data words. Write:

- a short title of the form `# <what the benchmark does>` (no
  `<STANDARD> <DOMAIN>` prefix in the title);
- a `Goal:` line naming the variables derived;
- an `Input:` line naming the source shape in plain words;
- a `Variables:` list with one bullet per output variable, in output order,
  saying what its value means and what it holds when the inputs do not
  support it;
- at most one closing `Note:` paragraph, for a rule that governs several
  variables at once;
- for every negative benchmark, a final `## How to fix` section that recommends
  the safest correction first and uses a short YAML snippet when it clarifies
  the change.

A `schema-*` benchmark explains spec behavior rather than deriving data. Its
`Input:` names the spec files and how they compose, and its `Note:` states the
behavior rule. It may include a `Variables:` list when output values make the
behavior concrete. It still names every non-key golden column somewhere in the
contract so the coverage check below stays silent.

Lifecycle, standard and domain belong in `benchmark.yaml`. Dashboards render
that metadata; README files contain only the data contract. New benchmarks
start at `draft`, as defined in `README.md`.

Keep bullets to the variables a reader must understand. Direct key copies and
fixed values need no bullet.

Before `## How to fix`, **do not name anything from the schema.** No rule IDs,
no expression or field names, no `output.columns`, and none of the words
*handler*, *verification*, *derivation*, or *schema*. State the effect instead:

    - counting neither the last day of period one nor the first day of period
      two, so consecutive periods give zero
    + `diff` with `bounds: between`

    - A subject who never entered period two has no exposure records there
    + Its period-two right side is empty after filtering, treated as
      an absent match

A positive benchmark with alternative specs may add a final `## Specification
variants` section after the data contract. Keep it to the variant filenames,
runtime operations, and the semantic difference between them; do not turn it
into a schema tutorial.

**Do not describe intermediate columns in the data contract.** A declared
column omitted from `output.columns` is not in the artifact and does not belong
before the fix section. A concise fix may show one when a multi-step correction
requires it.

Before `## How to fix`, **do not count the sample data.** "Two of the four
subjects have no exposure" is a property of the input file; "a subject with no
exposure falls back to the planned arm" is the rule. Write the rule. Row
counts, keys, and expected handler counts are stated in the specification and
do not belong in the data contract. A fix may name an offending sample value
when that makes the correction concrete.

In `## How to fix`, lead with the clinical or data decision. Then show the
smallest valid correction. Name exact fields and operations there when doing so
makes the remedy easier to apply. Distinguish alternatives only when they
represent genuinely different policies; do not turn the section into a general
tutorial.

Wrap prose and code at 79 columns. Most positive benchmarks fit in under 25
lines; negative benchmarks may be longer because they carry remediation.

## Design findings belong in the issue tracker

An benchmark that cannot express something is a design finding, and findings live
in this repository's issue tracker, grouped by root cause under one work
item each, so that one limitation is stated once and names the benchmarks
that show it.

Before removing a finding from a benchmark README, confirm the work item records
it and names the benchmark. If it does not, migrate it first. Deleting the only
statement of a limitation is the most common way this suite loses information.

When a gap closes, delete its statement from the work item rather than marking
it closed in place, and close the work item when its last gap closes.

**Rules and schema files never cite an issue.** A rule states the limitation
itself, so closing a gap means editing the rule text that states it, not
following a link out of the repository.

## The specification

- Declare `keys`, and verify what the derivation guarantees rather than what
  the sample data happens to show.
- Prefer no intermediate columns. When one is unavoidable, omit it from
  `output.columns`; it stays available to dependents and to verifications.
- Use `rows` when row construction changes the row count. A specification whose
  input already maps one-to-one to its output usually needs columns, not row
  templates. `sdtm-vs-units` is the deliberate exception, and it
  reorders its output to get per-test separation.
- Add `row.group_by` only when one candidate row must be constructed from a
  driver group. In that mode aggregate row derivations summarize the group and
  `row.filter` decides whether the completed candidate is emitted; without it,
  `row.filter` continues to select individual driver records before derivation.
- Declare columns in dependency order: source and independent columns first,
  then each column only after every declared column it reads. Preserve the
  artifact layout independently in `output.columns`.
- Keep the identifier columns `DOMAIN`, `STUDYID` and `USUBJID` in that order
  wherever a benchmark carries them: in the declared columns, in
  `output.columns` and in each golden CSV header, as the repository checks
  require. This deliberately departs from SDTMIG 3.4 section 4.1.4, which
  asks for the order SDTM v2.0 section 3.1.4 lists: `STUDYID`, `DOMAIN`,
  `USUBJID`. The benchmarks keep `DOMAIN` first for 1.0, since moving it
  would rewrite the expected files of every benchmark that carries it.

## Golden output

The expected file is a contract. Before committing a change to one, reproduce
it independently -- read the input, apply the rule by hand or in a short
script, and compare -- rather than accepting whatever the change produced.

Changing a golden file is a decision. Say in the pull request which values
moved and why, and confirm that every other benchmark's output is untouched.

## Expected failures and blocked benchmarks

An benchmark whose purpose is to fix failure behavior has
`expected/error.yaml`. It is a partial structured assertion over the failure
and has these fields:

- `phase`: the evaluation phase that rejects the run;
- `condition`: a stable snake-case name for the failed condition;
- `spec_paths`: one or more specification locations implicated in the failure;
- `rule`: the rule card the benchmark pins, such as `LOOKUP-9`;
- `context`: optional structured facts such as the dataset, offending keys,
  value, match count, or verification ID.

An implementation may report additional context and may word its human-readable
message differently. The expected fields and values must match. Stack traces
and implementation-specific exception classes do not belong in this artifact.

A `spec_paths` entry names the narrowest authored unit whose own contract
failed, rather than every location an author might edit to repair the failure.
A field-local failure names the exact field or list entry; an operation-wide
failure names the operation; and a conversion failure names the column whose
lifecycle could not complete. A failure involving several authored units names
each one once, in specification order. Do not add a parent or an absent field
merely because changing it would be another possible repair.

A [schema language](../rules/reference/schema-language.md) shorthand is the
only spelling of the value its long form would merely restate, and it expands
to its canonical class before the operation is prepared. A failure inside a
written shorthand names the operation or the field of that class the operation
reports: a bare string derivation fails at `derivation.source`, a lone
aggregate formula at `aggregate` and a lone flag condition at
`flag.condition`. Normalization never invents a location outside the expanded
class, and a written long form that only restates its shorthand fails at its
own path as `redundant_long_form`.

The primary condition and specification paths of a negative case stay solely
in the independent `expected/error.yaml`; `benchmark.yaml` carries no
validation block. Add `covers: [operation, "verify:operation"]` to the metadata for
vocabulary failure coverage; shared installed refusals declare `covers` beside
their existing cases in the shared host fixture owner.

`phase` comes from the two closed tables below, so two benchmarks that stop at
the same point say so the same way. Whole-run evaluation failures use:

| Phase | Rejects |
|---|---|
| `validation` | the specification itself, before any data is read |
| `ingest` | a stored value, against the type its field carries |
| `row_construction` | evaluating a row template |
| `derivation` | evaluating a column's expression over a row |
| `output` | output identity, once every column holds its final value |
| `verification` | a declared assertion |

A condition that an operation could have answered locally uses the stage in
the [local-handlers contract](../rules/columns.md#local-handlers):

| Phase | Rejects |
|---|---|
| `bind` | an absent source variable or ODM item |
| `join` | an unresolved multiple match, or a read that finds no record under a `require_match` its lookup, source or input declares |
| `mapping` | an unmapped lookup input, a pattern that matches nowhere, or a missing placeholder value |
| `impute` | invalid collected date or duration text |
| `convert` | a result that cannot take its declared type |
| `final` | a failed final replacement |

Declaring the corresponding handler is exactly what turns the failure into a
value, which is what makes the pairing worth keeping.

`condition` names what failed rather than what the implementation raised, and
one condition keeps one name across every benchmark that provokes it.

An expected CSV may be committed beside `error.yaml`. For a failure after the
dataset is completed, it is the dataset presented to the failing check. For an
earlier expressiveness failure, it is the intended artifact once the missing
capability exists. Neither is an accepted artifact from the current failed
run. Its README still describes the expected variables under the same data-only
contract as a positive benchmark.

Every negative README ends with exactly one `## How to fix` section. It
explains how to correct defective input and how to state an explicit policy
when more than one valid outcome exists. It must not recommend weakening a
check merely to make the sample pass.

## Checks to run before finishing

Use the supported setup and repository checks in [CONTRIBUTING.md](../CONTRIBUTING.md).
From the repository root, run a positive case through its declared Python
entries in a staged copy. Each entry is saved there before the next is built;
nothing is published into the checkout:

```sh
uv run --project python --no-sync python -m tools.benchmark benchmarks/<name>
uv run --project python --no-sync python -m tools.validation.validate_repository
uv run --project python --no-sync python -m tools.commands.lint benchmarks/<name>/
```

Conformance qualification compares both hosts with the authored expected data.
When prompts or independent solutions change, also run the Harbor controls in
the evaluation's own locked environment:

```sh
uv sync --project evaluations/harbor
uv run --project evaluations/harbor --no-sync pytest evaluations/harbor/tests
```

## Adding a benchmark

1. Write `benchmark.yaml`, `spec.yaml` or the applicable `spec_<variant>.yaml`
   files, the input
   data, and either the expected output or the expected error. Add an expected
   CSV beside an error when it makes a blocked or rejected result concrete.
2. Write the README to the contract above. A negative benchmark must include its
   `## How to fix` section.
3. Once the README of a positive `sdtm-*` or `adam-*` benchmark is
   `reviewed`, write its full prompt,
   `../evaluations/harbor/prompts/<name>/full.md`, to the
   [benchmark prompt recipe](../evaluations/harbor/prompts/recipe.md), and its
   `conventions.md`. The prompt is written from the README, specification
   and inputs, never `expected/`; the recipe's writing flow runs the writer
   where the expected data does not exist. The harness derives the brief
   prompt.
4. Record any finding it exposes as a gap on the matching work item in the
   issue tracker, or add the benchmark's name to the gap that already states it.

## Before deleting a benchmark

Some benchmarks are the only exercise of a construct or a rule, so removing one
silently drops coverage. Check what a benchmark uniquely covers before deleting
or merging it, and replace the coverage in the same change.

## Selftest cases

The benchmarks that `selftest.yaml` lists are the selftest cases (#3030 D105,
D108): the facade embeds their files when it builds, so every package ships
them. A change that adds, removes or renames a file of a listed case, or that
changes the list, reruns
`uv run --project python --no-sync python -m tools.packaging.generate_selftest_cases --write`
in the same change; `make quick` fails until the facade's table matches.

A shipped file carries no third-party licence, so no case copies an upstream
fixture. The SAS transport input of `schema-source-xpt` is written by
`python -m tools.staging.build_xport_fixtures` from the literal table there,
from the public record layout, without ReadStat or the engine's reader; change
that table and rerun it rather than editing the file.

## Adding an expression

An expression enters the vocabulary when a benchmark needs it, a negative
benchmark fixes its failure behavior, and R and Python can implement it the same
way. Sponsor-specific algorithms stay behind `function`.

Prefer one closed grammar to an entry per operator: `compute` states the whole
numeric grammar once rather than registering `add`, `subtract`, and `divide`.

Prefer widening an existing entry to adding a new one -- but the test is the
kind of value, not the saving in YAML. Widen when the new behavior returns the
same kind of value, as `diff`'s `bounds` still returns a count. Add an
entry when it does not: `study_day` returns an ordinal on a calendar with no
zero, and folding it into `diff` would have allowed `unit: week` with it.

After registering one, update every place that enumerates the vocabulary: the
expression registry in the owning `../yaml/schema_*.yaml` module and the
generated schema-fields reference it feeds (see `../rules/README.md`), the
owning operation contract's type
behavior, and the [local-handlers contract](../rules/columns.md#local-handlers)
if it declares handlers. Then delete the gap it closed from its work item in the
issue tracker, and close the work item when its last gap closes.
