# Task spec

The contract between the human director and the research agent for one
benchmark-factory run. It has two halves: the fixed rules, which are the
same every run, and the human direction, which the human fills in per
run. AutoBenchmark's finding motivates the split: a one-sentence intent
barely improves on the autonomous baseline, while a detailed spec plus
curated grounding material roughly halves solver scores. Put the effort
in the second half.

## Work item types

A run either creates a benchmark or maintains existing ones. Name the
type first; the rest of the spec is read against it.

- **create**: a new benchmark closing a coverage gap. The existing
  behavior; everything below applies as written.
- **enhance**: extend an existing benchmark in place: new variables,
  new edge-case rows, harder fixtures. The golden may gain columns
  or rows, but every value the work item does not intend to change
  must stay byte-identical. An enhancement resets the lifecycle
  badge to `draft`: the content changed, so it is re-reviewed.
- **combine**: merge two or more benchmarks into one using the
  `spec_<variant>.yaml` convention. Coverage must not shrink: every
  rule the absorbed benchmarks pinned stays pinned. Absorbed
  directories are removed only after `factory.sh retire-check` is
  clean on each and a human approves.
- **retire**: remove a benchmark. Requires a clean
  `factory.sh retire-check`, a recorded reason, and a human
  decision. The agent prepares the evidence; it never deletes.
- **recalibrate**: re-run the difficulty pilot on an existing
  benchmark and harden or clarify from the results, without
  changing what it measures. Fixture-only changes; the golden
  values for existing rows do not move.

## Fixed rules (do not edit per run)

The proposed benchmark must satisfy, in order:

1. **Layout.** `benchmarks/<name>/` follows `../benchmarks/agents.md`:
   `README.md`, `spec.yaml` (or the applicable `spec_<variant>.yaml`
   files), `input/*.csv`, `expected/*.csv` (or `expected/error.yaml`
   for a negative benchmark), `run.py` and `run.R` for positive
   benchmarks.
2. **Naming.** The directory names what the benchmark derives, not the
   construct it uses (`adam-adlb-bds`, not `adam-adlb-rows`).
   `negative-*` pins a failure; `schema-*` exercises the spec language
   itself.
3. **README contract.** The data-contract portion describes effects in
   study-data words: no schema vocabulary, no rule IDs, no handler or
   derivation names, no counting the sample data. State rules, not
   rows. Negative benchmarks end with exactly one `## How to fix`
   section that leads with the clinical or data decision.
4. **Prompt recipe.** Positive `sdtm-*`/`adam-*` benchmarks get
   `evaluations/harbor/prompts/<name>/full.md` per
   `../automation/benchmark_prompt.md`: say what each value is, never
   how to compute it. The prompt plus the inputs must suffice to
   reproduce every golden cell, in either R or Python.
5. **Golden discipline.** The expected file is an independent contract.
   Reproduce it by hand or in a short script and compare; never accept
   whatever the engine produced. A golden change names which values
   moved and why.
6. **Changed-input robustness.** The Harbor grader renames every
   subject identifier and alters supported ages, measurements, and
   complete dates, then recomputes the answer with the reference. The
   prompt must therefore state rules, never sample facts: nothing in
   it may depend on a particular `USUBJID`, age, or date appearing in
   the fixtures.
7. **No leakage.** The solver sees the prompt and `/app/input/` only.
   The prompt must not reveal the specification's structure, name
   derivation verbs, or otherwise hand over the answer's shape.
8. **Maintenance golden discipline.** For enhance/combine/
   recalibrate, diff the golden before and after: every moved value
   is named in the proposal report with its reason, per
   `../benchmarks/agents.md` ("changing a golden file is a
   decision"). Unmoved values must be byte-identical; verify with
   `git diff`.
9. **Prompt parity.** After any maintenance change, the Harbor
   prompt (`full.md`, `conventions.md`, `brief.md`) must still ask
   for exactly the datasets and columns the golden holds. Rebuild
   `brief.md` with `evaluations/harbor/brief.py`; the repo's CI
   checks the rest.
10. **Reference solutions stay fresh.** The R and Python reference
    solutions in `evaluations/harbor/solutions/<name>/` are the
    oracle: they must produce the golden and score 1. Any
    maintenance change to the golden's columns updates them too.

## Human direction (fill in per run)

### Work item

_Type (create / enhance / combine / retire / recalibrate), the target
benchmark(s), and the one-paragraph change. For create, this section
is "what to measure" as below; for the maintenance types, name the
benchmarks and the intended end state._

### What to measure

_One paragraph: the derivation pattern or failure behavior this
benchmark pins, and why it matters. Name the CDISC standard and
domain, the variables derived, and the rule the golden depends on
that CDISC practice alone does not settle._

### Grounding material

_Curated, not scraped. List each source with one line on what the
agent may take from it:_

- _Existing benchmarks to imitate (directory names)._
- _Rules or schema docs that govern the pattern (paths under `yaml/`)._
- _External references (CDISC guidance section, pharmaverse/admiral
  function, paper) with the exact derivation logic quoted or
  summarized._
- _The example input data you curated, and which edge cases it covers
  (missing values, ties, boundary values, partial dates)._

### Difficulty target

_What should make this benchmark hard for a solver? Name 2-4 concrete
traps: e.g. "two subjects share a baseline date; the tie-break is the
earlier visit", "one lab result arrives in non-standard units"._

### Out of scope

_What the benchmark must not test, to keep it focused and to avoid
duplicating an existing benchmark (name the near-duplicates and the
line between them)._

### Acceptance sketch

_What the human gate will look for at Stage 5: the 2-3 properties that
make this benchmark worth finalizing._
