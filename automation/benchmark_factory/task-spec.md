# Task spec

The contract between the human director and the research agent for one
benchmark-factory run. It has two halves: the fixed rules, which are the
same every run, and the human direction, which the human fills in per
run. AutoBenchmark's finding motivates the split: a one-sentence intent
barely improves on the autonomous baseline, while a detailed spec plus
curated grounding material roughly halves solver scores. Put the effort
in the second half.

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

## Human direction (fill in per run)

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
