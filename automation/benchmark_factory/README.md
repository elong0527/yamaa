# Benchmark factory

An agentic loop for creating and maintaining yamaa benchmarks. It
adapts the AutoBenchmark recipe
(https://facebookresearch.github.io/RAM/blogs/autobench/) to this
repository: a research agent drafts a benchmark change, solvers
attempt it, and judges review it, with a human directing what to
build and gating what ships.

A run's work item is either a **creation** (a new benchmark closing
a coverage gap) or **maintenance** of existing benchmarks: enhance
(new variables or edge cases), combine (merge overlaps into
variants), retire (remove with evidence), or recalibrate (harden
or clarify from pilot results).

## Why this fits yamaa

AutoBenchmark asks a research agent to emit a Harbor-compatible
evaluation package: task instructions, the evidence the solver sees, a
reference solution, and machine-checkable grading criteria. That is
already what `evaluations/harbor/build.py` assembles from a directory
under `benchmarks/`. Two yamaa properties make the loop stronger here
than in the paper:

- **The engine is a free correctness oracle.** The agent proposes
  `spec.yaml`; running it through the engine produces the artifact the
  golden file must match. Correctness is mechanical, not LLM-judged.
- **The verifier already detects reward hacking.** The paper holds out
  solvers to check that difficulty transfers. `evaluations/harbor`
  goes further: the held-out rerun drops every third subject and the
  changed-input challenge renames subjects and alters values, so a
  script that hard-codes rows fails. A generated benchmark inherits
  both checks unchanged.

The paper's headline finding shapes the loop below: fully autonomous
runs produce saturated, low-validity benchmarks, while fine-grained
human direction (a detailed spec plus curated grounding material)
roughly halves solver scores. The human therefore owns *what* to build
and *whether* it ships; the agent owns the drafting and the revisions.

## The loop

```
Stage 0  HUMAN DIRECTION      work item: create / enhance / combine /
                              retire / recalibrate (see Work items); for
                              create, curate example data
        |
Stage 1  PROPOSAL             research agent drafts the change:
                              a new benchmarks/<name>/, or edits to
                              existing ones (see Proposer protocol)
        |
Stage 2  MECHANICAL ADMISSION repo validators, the engine, drift
                              checks, oracle/nop smoke; failures go
                              back to Stage 1 with the log attached
        |
Stage 3  DIFFICULTY PILOT     run 1-2 solver models on the candidate;
                              saturated  -> harden fixtures (Stage 1)
                              models fail, oracle passes -> clarify the
                              prompt (Stage 1)
        |
Stage 4  JUDGE REVIEW         LLM judges score the five criteria below
                              (Judge review), plus the maintenance checks
                              for enhance/combine/retire; revise or
                              reject accordingly
        |
Stage 5  HUMAN GATE           lifecycle draft -> reviewed -> finalized
                              stays a human decision; so does any
                              retire or combine
```

Stages 2 and the Harbor half of Stage 3 reuse existing machinery; this
folder supplies the parts the repository does not have yet: the task
spec template, the proposer instructions, the judge rubrics, and the
scaffolding script.

## Files

| File | Role |
|---|---|
| This README | The whole factory guide: work-item types and fixed rules, the proposer protocol, and the judge criteria. Benchmark layout, README and golden contracts themselves live in `benchmarks/AGENTS.md`, which outranks this page. |
| `factory.sh` | Portable scaffolding and checks: `scaffold`, `validate`, `packet`, `retire-check`, `drift-check`. |
| `references.md` | Curated external grounding sources (PharmaSUG, pharmaverse, CDISC TAUGs, FDA/PHUSE, ODM-to-SDTM/define.xml/ARM): work items cite these when claiming real-world relevance. |

The dated gap report that used to live here was retired in #2408: its
still-open proposals moved to issue #2477, and its completed or rejected
items remain in Git history and their original issues.

## Running one cycle

1. Open the work-item issue (title `[work-item] <benchmark>: <change>`,
   label `work-item`, body from the Human direction slots below).
   For a creation, curate the example input data yourself: the paper
   shows this is the highest-leverage input. For maintenance, name
   the target benchmark(s) and the intended end state.
2. Hand the work-item issue and this README to a research agent. It
   writes a new `benchmarks/<name>/` or edits existing ones,
   following `../../benchmarks/AGENTS.md`.
3. `./factory.sh validate <name>` runs the mechanical admission
   (Stage 2). Fix or regenerate until clean.
4. Build the Harbor task and pilot it (Stage 3):
   `python evaluations/harbor/build.py --benchmarks <name> ...` then
   `python evaluations/harbor/smoke.py`. The pilot needs Docker and a
   model key; it is the only stage that leaves this machine.
5. Score the candidate against Judge review below (Stage 4), revise, and keep
   the best accepted revision as the checkpoint for the next round.
6. Open the pull request. A human moves the lifecycle badge from
   `draft` to `reviewed` to `finalized` (Stage 5). PRs from
   maintenance work items use the `[benchmark-maintain]` title
   prefix.

## What the loop does not do

- It never rewrites a golden file from the engine's output. A golden
  is an independent contract: reproduce it by hand or in a short
  script and compare, per `../../benchmarks/AGENTS.md`. `factory.sh` has
  no regenerate-expected command on purpose.
- It never writes the grader. `evaluations/harbor/grade.py` is reused
  unchanged; the agent's job is the task content, not the rubric.
- It never finalizes. The `finalized` badge is a human decision, as is
  any retirement proposed in a work item.

## Portability

Everything the loop needs besides the repository itself is in this
folder: this guide, the reference library, and one POSIX shell script
with no dependencies beyond what the repo already uses (`uv`, `python3`,
`git`). The Docker/model stages shell out to `evaluations/harbor` and
degrade gracefully when those are unavailable: Stages 0-2 and 4 run
anywhere.

## Work items

One GitHub issue per run: title `[work-item] <benchmark>: <change>`,
label `work-item`. The issue is the work item; nothing about it lives in
the repo. Name the type first; the rest is read against it.

- **create**: a new benchmark closing a coverage gap.
- **enhance**: extend an existing benchmark in place (new variables,
  edge-case rows, harder fixtures). Values the work item does not intend
  to change stay byte-identical; the lifecycle badge resets to `draft`.
- **combine**: merge benchmarks into one using the `spec_<variant>.yaml`
  convention. Coverage must not shrink; absorbed directories are removed
  only after `factory.sh retire-check` is clean on each and a human
  approves.
- **retire**: remove a benchmark. Requires a clean
  `factory.sh retire-check`, a recorded reason, and a human decision.
  The agent prepares the evidence; it never deletes.
- **recalibrate**: re-run the difficulty pilot and harden or clarify
  from the results without changing what the benchmark measures.
  Fixture-only changes; existing golden values do not move.

### Fixed rules

The layout, naming, README, prompt and golden contracts are the ones in
`benchmarks/AGENTS.md`; this page does not restate them. In addition:

- The prompt plus the inputs must suffice to reproduce every golden
  cell, in either R or Python, and must state rules, never sample facts
  (the Harbor changed-input challenge renames subjects and alters
  values, then recomputes with the reference).
- The solver sees the prompt and `/app/input/` only: no schema
  vocabulary, derivation verbs, or specification structure in the prompt.
- After any maintenance change, the Harbor prompt files
  (`full.md`, `conventions.md`, `brief.md`) still ask for exactly the
  datasets and columns the golden holds, and the R and Python reference
  solutions in `evaluations/harbor/solutions/<name>/` still produce the
  golden and score 1.

### Human direction (the work-item issue body)

Fill in, per run: the type, target benchmark(s) and one-paragraph
change; what it measures (the derivation pattern or failure behavior,
the CDISC standard and domain, the variables, and the rule CDISC
practice alone does not settle); grounding material, curated not
scraped — start from `references.md` and cite its entries by name, list
the benchmarks to imitate, the governing rules/schema docs, the external
references, and the example input data with its edge cases; the
difficulty target (2–4 concrete traps); what is out of scope (name the
near-duplicates and the line between them); and an acceptance sketch
(the 2–3 properties the human gate will look for). AutoBenchmark's
finding motivates the effort here: a detailed spec plus curated
grounding roughly halves solver scores, while a one-sentence intent
barely improves on the autonomous baseline.

## Proposer protocol

The research agent drafts exactly one benchmark change from a filled-in
work item, then revises it against solver and judge feedback. The fixed
rules and `benchmarks/AGENTS.md` outrank everything else it reads.

1. Choose the narrowest scope that still pins the pattern; write the
   fixtures first, covering every edge case the work item lists.
2. Write the spec, run the engine, and independently reproduce the
   golden by hand or in a short throwaway script, comparing cell by
   cell. Never ship the engine's raw output as the golden.
3. Write the README and the Harbor prompt to their contracts, then run
   `./factory.sh validate <name>` and fix everything it reports.
4. On revision, read solver trajectories before scores; never fix
   difficulty by making the task impossible, ambiguous, or dependent on
   unstated conventions — the oracle must keep scoring 1 and the
   changed-input challenge must keep passing. Keep a short changelog of
   what changed per revision and which feedback motivated it.

Deliverables: the complete `benchmarks/<name>/` directory, the prompt
files, a clean validation log, and a one-page proposal report (what it
measures, its grounding, the edge cases and where they live in the
fixtures, the revision changelog). For maintenance work, read the target
benchmark(s) end to end first and quote the lines the change affects;
for enhance, confirm unmoved golden values with `git diff` on
`expected/` and update the prompts and both reference solutions when
columns change.

Anti-patterns: a benchmark the proposer itself cannot solve from the
prompt and inputs alone; edge cases all in one subject (the held-out
rerun drops every third subject); prompts that restate a negative
benchmark's `## How to fix` or name yamaa verbs; golden files with
`NA`, `.`, or empty cells where a value is meant; new vocabulary
introduced to serve one benchmark.

## Judge review

Score each criterion **pass**, **revise**, or **fail**, with one quoted
piece of evidence (a fixture row, a prompt sentence, a spec line) per
non-pass. A single **fail** rejects the revision; any **revise** sends
it back to the proposer with the evidence attached. All five **pass**
and the revision is accepted as the new checkpoint. `factory.sh packet
<name>` assembles the review packet, including this verdict form.

1. **Construct validity** — the pattern appears in CDISC guidance,
   pharmaverse/admiral, or production practice, and no existing
   benchmark pins the same rule. A second unrelated idea is a revise
   (split it); fixture trivia or duplication is a fail.
2. **Correctness** — the entry spec runs clean, its output matches
   `expected/` cell by cell, and the golden was independently
   reproduced; for a negative benchmark, the engine rejects the spec in
   the declared phase and `expected/error.yaml` matches on `phase`,
   `condition`, `spec_paths`, and `requirement`.
3. **Feasibility** — a reference solution written from `full.md` plus
   `input/` scores 1 and the changed-input challenge passes. Genuine
   ambiguity two reasonable readers would resolve differently is a
   revise; unsolvable-as-stated is a fail.
4. **Usefulness** — it closes a named gap (vocabulary coverage, the
   validation manifest, an open issue, or the work item's acceptance
   sketch) and the difficulty pilot discriminates between solvers.
5. **No leakage, no gaming** — the prompt never names schema vocabulary,
   derivation verbs, or the spec's structure, and the README's
   data-contract portion obeys the prose rules in
   `benchmarks/AGENTS.md`.

Maintenance checks, in addition: prompt parity (the prompt asks for
exactly what the new golden holds; reference solutions derive the new
columns too) and, for combine, no lost coverage (every absorbed
benchmark's edge cases retain a witness). A retire verdict of accept
needs a clean `factory.sh retire-check`, a recorded reason, and an
explicit human decision; the judge prepares the first two and never
supplies the third.
