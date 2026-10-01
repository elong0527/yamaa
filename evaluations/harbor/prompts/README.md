# Prompts

The requests an agent gets in a Harbor task, one folder per benchmark,
`<benchmark>/<tier>.md`. A benchmark's tiers ask for the same datasets and
are graded against the same expected data; they differ only in how much of
the rules the prompt states, so comparing one model's runs across tiers
measures what the stated rules are worth.

| File | Tier | What it keeps | Question it answers |
|---|---|---|---|
| `full.md` | full | the opening, the column list, every rule an expected cell depends on, and the paths | Can the agent follow a complete specification? |
| `brief.md` | brief | the opening, the column list, and the paths | How much of the specification can the agent rebuild from CDISC practice, the column names, and the data? |

## full

The complete request a statistician would send a statistical programmer,
written by hand to the
[benchmark prompt recipe](../../../automation/benchmark_prompt.md). It has
four parts, in order:

1. **Opening.** The standard, the input datasets, and the record level
   (`Following CDISC ADaM standards, use the provided TRT and EX datasets
   to create an ADEX dataset with one record per subject per treatment.`),
   with a definition when the record level needs one (a responder).
2. **Column list.** Every output column in order, one list per dataset.
3. **Rules.** Labels, codes, key numbering, constants, precision, and the
   study rules: what counts as an event, which source wins, how ties
   break, and what a variable holds when its inputs give no value.
4. **Paths.** Where the inputs are and where each dataset is saved.

The full prompt is the source of every other tier, and the one
[`../build.py`](../build.py) builds Harbor tasks from. Change it in the same
change as any edit to its benchmark's README, inputs, or expected data.

## brief

The full prompt without its rules: parts 1, 2, and 4, verbatim. It is
written by [`../brief.py`](../brief.py), never by hand; rerun it after
changing a full prompt.

The brief prompt is not meant to be solvable everywhere. The expected data
still holds the sponsor's choices that only the rules state: category
labels and codes, how a sequence number runs, how an identifier is formed,
and what a derived variable holds at an edge. An agent can reason its way
to the standard ones and must guess the rest. Read a brief run against a
full run of the same model on the same tasks, column by column, not on its
own.

## Rules for every tier

- **A tier only removes.** It never states anything the full prompt does
  not, so it can leak nothing the full prompt withholds.
- **The request stays the same.** Every tier keeps the full prompt's
  opening, column list, and paths verbatim, so every tier asks for the same
  datasets, columns, and files.
- **Everything else stays the same.** Inputs, expected data, the system
  prompt for the language, and grading are shared by every tier.
- **A tier's meaning is fixed here.** Changing what a tier keeps makes a
  new tier with a new name, since runs on the old one would no longer
  compare. A new tier, such as one that keeps the sponsor's conventions
  but drops the study rules, is defined in this file before any task is
  built from it.

## Checks

`python/tests/test_harbor_evaluation.py` checks that:

- every `sdtm-*` and `adam-*` benchmark has a folder here, holding
  `full.md` and `brief.md`, and no prompt is left under `benchmarks/`;
- every full prompt has the four parts, and every brief prompt is exactly
  what `brief.py` writes from it;
- every tier of a graded benchmark asks for exactly its output files and
  column lists, as the expected data holds them;
- every tier keeps to the recipe's 79-column width and never names the
  harness (yamaa, specifications, rule identifiers).

The Harbor Evaluation workflow (`.github/workflows/harbor-evaluation.yml`)
runs these checks, and every reference solution in both languages, on each
pull request that changes a benchmark or the evaluation.
