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
                              retire / recalibrate (task-spec.md); for
                              create, curate example data
        |
Stage 1  PROPOSAL             research agent drafts the change:
                              a new benchmarks/<name>/, or edits to
                              existing ones (proposer.md)
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
Stage 4  JUDGE REVIEW         LLM judges score the five criteria in
                              judges.md, plus the maintenance checks
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
| `task-spec.md` | The Stage 0/1 contract: work-item types, fixed rules, and the per-run human direction slots. |
| `proposer.md` | Instructions for the research agent that drafts a benchmark or a maintenance change. |
| `judges.md` | The five review criteria adapted to yamaa, plus maintenance checks, with verdict format. |
| `factory.sh` | Portable scaffolding and checks: `scaffold`, `validate`, `packet`, `retire-check`, `drift-check`. |
| `gap-report.md` | Sourced proposals: new benchmarks to add, and existing ones to enhance, combine, or retire. |
| `references.md` | Curated external grounding sources (PharmaSUG, pharmaverse, CDISC TAUGs, FDA/PHUSE, ODM-to-SDTM/define.xml/ARM): work items cite these when claiming real-world relevance. |
| `work-items/` | Filled-in task specs, one per run: the factory's work log. |

## Running one cycle

1. Write the work item (`work-items/<slug>.md` from `task-spec.md`).
   For a creation, curate the example input data yourself: the paper
   shows this is the highest-leverage input. For maintenance, name
   the target benchmark(s) and the intended end state.
2. Hand the work item and `proposer.md` to a research agent. It
   writes a new `benchmarks/<name>/` or edits existing ones,
   following `../benchmarks/agents.md`.
3. `./factory.sh validate <name>` runs the mechanical admission
   (Stage 2). Fix or regenerate until clean.
4. Build the Harbor task and pilot it (Stage 3):
   `python evaluations/harbor/build.py --benchmarks <name> ...` then
   `python evaluations/harbor/smoke.py`. The pilot needs Docker and a
   model key; it is the only stage that leaves this machine.
5. Score the candidate against `judges.md` (Stage 4), revise, and keep
   the best accepted revision as the checkpoint for the next round.
6. Open the pull request. A human moves the lifecycle badge from
   `draft` to `reviewed` to `finalized` (Stage 5). PRs from
   maintenance work items use the `[benchmark-maintain]` title
   prefix.

## What the loop does not do

- It never rewrites a golden file from the engine's output. A golden
  is an independent contract: reproduce it by hand or in a short
  script and compare, per `../benchmarks/agents.md`. `factory.sh` has
  no regenerate-expected command on purpose.
- It never writes the grader. `evaluations/harbor/grade.py` is reused
  unchanged; the agent's job is the task content, not the rubric.
- It never finalizes. The `finalized` badge is a human decision, as is
  any retirement proposed in `gap-report.md`.

## Portability

Everything the loop needs besides the repository itself is in this
folder: two markdown contracts, one rubric, and one POSIX shell script
with no dependencies beyond what the repo already uses (`uv`, `python3`,
`git`). The Docker/model stages shell out to `evaluations/harbor` and
degrade gracefully when those are unavailable: Stages 0-2 and 4 run
anywhere.
