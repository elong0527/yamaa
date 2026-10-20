# Benchmark proposals and review

Use this workflow to create, enhance, combine, retire or recalibrate a benchmark.
[AGENTS.md](../../AGENTS.md) supplies repository rules;
[benchmarks/AGENTS.md](../../benchmarks/AGENTS.md) owns benchmark layout, metadata,
authoring and expected-data discipline. The
[contributor guide](../../CONTRIBUTING.md) owns verification commands. This page
adds the proposal and review process, rather than another validator.

Candidate coverage gaps live in [the issue backlog](https://github.com/elong0527/yamaa/issues/2477).
Check current benchmark contents before proposing a gap or overlap: a directory
name alone cannot establish missing or redundant coverage.

## Work item

Open a focused issue containing:

- The change type and target benchmarks, with one derivation or failure pattern
  to measure. Name the intended output, standard and clinical domain.
- The public rule and exact external source/version supporting the derivation.
  State sponsor-specific decisions explicitly rather than assuming them.
- Nearby benchmarks and the behavior that makes the proposal distinct.
- Independently invented inputs, including the missing values, ties, boundaries
  or incomplete dates needed to distinguish correct behavior.
- The scope exclusions and concrete acceptance criteria.

Useful source entry points are [CDISC standards](https://www.cdisc.org/standards),
[admiral](https://pharmaverse.github.io/admiral/reference/),
[admiralonco](https://pharmaverse.github.io/admiralonco/),
[PHUSE scripts](https://github.com/phuse-org/phuse-scripts) and the
[synthetic submission pilot](https://github.com/RConsortium/submissions-pilot7-synthetic-data).
Cite the specific material actually used in the issue. Recheck proposals in the
recovered gap report against current benchmarks and the issue backlog.

## Recovered factory materials

The following files preserve the factory workflow from before the contributor
guide consolidation. Their contents are restored as they were before removal.

| File | Contents |
| --- | --- |
| [task-spec.md](task-spec.md) | Work item types, fixed rules and human direction template. |
| [proposer.md](proposer.md) | Proposal and revision instructions for the research agent. |
| [judges.md](judges.md) | Review criteria, verdict format and maintenance checks. |
| [references.md](references.md) | Curated external grounding sources. |
| [gap-report.md](gap-report.md) | Historical coverage proposals to reassess against the current suite. |
| [factory.sh](factory.sh) | Original scaffolding, validation and review packet helper. |

These recovered files describe an earlier repository layout. In particular,
`factory.sh` expects per-case runners and retired test paths, so it is retained
for reference rather than supported validation. Use [benchmarks/AGENTS.md](../../benchmarks/AGENTS.md)
for current metadata and fixture requirements, the
[contributor guide](../../CONTRIBUTING.md) for supported commands, and the
[prompt recipe](../../evaluations/harbor/prompts/recipe.md) for prompt authoring.

## Draft and validate

Read the target benchmarks end to end, including expected artifacts and their
Harbor materials. Write the inputs and independently reproduce expected values
from the stated rule. Then implement the specification and compare its result
with that independent truth. Engine output cannot establish a golden value.

Follow the benchmark area guide for mechanical checks and the
[Harbor guide](../../evaluations/harbor/README.md) for prompt, reference-solution
and changed-input checks. Write prompts with the writing flow of the
[prompt recipe](../../evaluations/harbor/prompts/recipe.md): the writer works
from the README, specification and inputs in a sandbox without the expected
files, and the reference solutions come from solvers that saw only the
prompt and inputs.
Keep relevant oracle and grading controls unchanged while editing task content.

When a maintenance change adds rows or columns, preserve existing expected values
except where the work item explicitly changes their meaning. Name every moved
value and its independent justification in the PR. Update the data description,
metadata, prompts and both host reference solutions together. Reset changed
content to the draft lifecycle state defined by the benchmark area guide.

For a combine or retire proposal, map every original behavior and edge case to
its surviving witness. Search repository references before deleting a directory;
update all callers and metadata in the same change. No original expected data
may disappear merely because two case names sound similar.

## Difficulty and review

Where agent evaluation is relevant, pilot the candidate using the Harbor
workflow. Inspect trajectories as well as scores. If solvers all pass, add a
meaningful edge case; if a reference passes while solvers fail on ambiguous
instructions, clarify the prompt. Keep the best reviewed revision rather than
building on a rejected one. Do not manufacture difficulty through ambiguity or
hidden sponsor conventions.

Review each criterion as **pass**, **revise** or **fail**, citing a concrete
fixture, source or prompt line:

| Criterion | Evidence required |
| --- | --- |
| Validity | A real, sourced derivation or failure pattern distinct from existing coverage. |
| Correctness | Independently reproduced expectations and exact positive or negative results. |
| Feasibility | Reference solutions can derive the full result from the prompt and inputs alone. |
| Usefulness | A named coverage gap and, where measured, informative solver behavior. |
| No leakage | No answer or implementation recipe in task materials; changed-input controls pass. |

A failure rejects the revision; a revise verdict identifies the concrete next
change. A passing technical review supports the lifecycle decision described in
the benchmark guide. Record the decision and validation in the PR, not in a
checked-in review packet, run ledger or generated report.
