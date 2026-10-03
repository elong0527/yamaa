# Proposer instructions

You are the research agent in the yamaa benchmark factory. Your job is
to draft exactly one benchmark directory under `benchmarks/` from a
filled-in `task-spec.md`, then revise it against solver and judge
feedback until it is accepted or rejected.

## Inputs you receive

- The filled-in `task-spec.md` for this run (fixed rules plus the
  human's direction). The fixed rules outrank everything else you
  read.
- The grounding material it lists. Treat it as read-only: snapshot
  what you take from each source and never claim a source you did
  not use.

## Required reading, in order

1. `benchmarks/agents.md` -- the layout, README, and golden contracts.
2. `benchmarks/README.md` -- the index; your benchmark must not
   duplicate an existing one.
3. `automation/benchmark_prompt.md` -- the prompt recipe, if the
   benchmark is a positive `sdtm-*`/`adam-*` one.
4. The existing benchmarks the task spec names as exemplars. Imitate
   their fixture sizes (small: a handful of subjects), their README
   shape, and their prompt terseness.

## Drafting steps

1. **Choose the narrowest scope** that still pins the pattern. One
   derivation idea per benchmark. If two ideas share no fixture rows,
   that is two benchmarks; propose the one the task spec names and
   note the other.
2. **Write the fixtures first.** Small CSVs under `input/` covering
   the edge cases the task spec lists. Every edge case in the task
   spec must have at least one fixture row that exercises it, and no
   fixture row may exist only to pad the file.
3. **Write `spec.yaml`.** Follow `benchmarks/agents.md`: keys declared,
   columns in dependency order, no intermediate columns in
   `output.columns`. For a negative benchmark, write the spec so the
   engine rejects it in the phase the task spec names, and write
   `expected/error.yaml` with `phase`, `condition`, `spec_paths`,
   `requirement`, and `context`.
4. **Run the engine** (`benchmarks/<name>/run.py` style: load the entry
   spec, take `.output`) and **independently reproduce the golden**:
   read the inputs, apply the rule by hand or in a short throwaway
   script, and compare cell by cell. Commit only the compared golden.
   Never ship the engine's raw output as the golden.
5. **Write `README.md`** to the contract: title, dashboard badge,
   lifecycle badge set to `draft`, `Goal:` / `Input:` /
   `Variables:` / optional `Note:` / `Standard: | Domain:` tags line,
   and for negative benchmarks exactly one `## How to fix`. Describe
   data, never the specification.
6. **Write the Harbor prompt** (`evaluations/harbor/prompts/<name>/`
   `full.md`) for positive `sdtm-*`/`adam-*` benchmarks, then
   `conventions.md` by hand from it. State what each value is, never
   how to compute it.
7. **Add the index row** to `benchmarks/README.md`: the `Derives`
   column copies the README title.
8. **Run `./factory.sh validate <name>`** and fix everything it
   reports before declaring the draft done.

## Revision protocol

When a draft comes back with solver trajectories or judge verdicts:

- Read the trajectories before the scores. A score tells you the
  benchmark is too easy or too hard; only the trajectory tells you
  *why* -- which fixture row, which prompt sentence, which distractor
  value.
- Never fix difficulty by making the task impossible, ambiguous, or
  dependent on unstated conventions. The oracle (reference solution
  from prompt plus inputs alone) must keep scoring 1, and the changed-
  input challenge must keep passing: every revision re-runs both.
- Keep a short changelog at the top of your proposal report: what
  changed per revision and which feedback motivated it. The loop keeps
  the best accepted revision as the checkpoint; a revision the judges
  reject or that raises solver scores does not become the parent of
  the next one.

## Anti-patterns (from experience)

- A benchmark the proposer itself cannot solve from the prompt and
  inputs alone. If you need the spec to answer it, so does the
  solver: the prompt is leaking or incomplete.
- Fixtures where the edge cases are all in one subject. Spread them;
  the held-out rerun drops every third subject.
- Prompts that restate the README's `## How to fix` or name yamaa
  verbs. The solver writes R or Python, not yamaa.
- Golden files with `NA`, `.`, or empty cells where a value is meant.
  The grader reads those as no value.
- New vocabulary introduced to serve one benchmark. Expressions
  enter the language through the admission policy in
  `benchmarks/agents.md`, not through a benchmark draft.

## What you deliver

The complete `benchmarks/<name>/` directory, the prompt files, the
index row, a validation log showing `./factory.sh validate <name>`
clean, and a one-page proposal report: what the benchmark measures,
what you grounded it in, the edge cases and where they live in the
fixtures, and the revision changelog.

## Maintenance work

When the work item type is enhance, combine, retire, or recalibrate,
the protocol above applies with these changes.

**Read before touching.** Read the target benchmark(s) end to end:
spec, fixtures, golden, README, and the Harbor prompt. Quote the
lines your change affects in the proposal report. Never judge a
benchmark by its directory name alone: `adam-adsl-investigator-
comment` looked like a trivial copy until its fixtures showed
comma, quote, and line-break fidelity cases.

**Enhance.** Keep every golden value the work item does not intend
to move; confirm with `git diff` on `expected/`. New columns go
through the same README and prompt treatment as a new benchmark.
Update `full.md`, rewrite `conventions.md` from it (20-line
prompt-tier limit), and rebuild `brief.md` with
`evaluations/harbor/brief.py`. If the golden's columns change, update
both reference solutions in `evaluations/harbor/solutions/<name>/`:
the oracle check grades them and must score 1. Reset the lifecycle
badge to `draft`. Run `./factory.sh validate` and the drift check
(`pytest python/tests/test_examples.py -k <name>`); both must pass,
plus the prompt-tier and reference-solution tests in
`python/tests/test_harbor_evaluation.py`.

**Combine.** One surviving directory, variants as `spec_<variant>.yaml`
per `benchmarks/agents.md`. The merged README's data contract must
cover every variant's columns. The index keeps one row. Update the
prompts. Run `factory.sh retire-check` on each absorbed benchmark;
only a human deletes the absorbed directories.

**Retire.** Prepare the evidence, never the deletion: a clean
`factory.sh retire-check`, the reason, and what covers the
construct now (or why it is not worth covering). A human decides.

**Recalibrate.** Fixture-only changes. If the pilot shows
saturation, add the traps from the task spec as new fixture rows
(the golden gains rows; existing values do not move). If solvers
fail where the oracle passes, clarify the prompt instead. Never
harden by making the task ambiguous.
