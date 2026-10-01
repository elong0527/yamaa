# Prompts

The requests an agent gets in a Harbor task, one folder per benchmark,
`<benchmark>/<tier>.md`. A benchmark's tiers ask for the same datasets and
are graded against the same expected data; they differ only in how much of
the rules the prompt states, so comparing one model's runs across tiers
measures what the stated rules are worth.

| File | Tier | What it keeps | Question it answers |
|---|---|---|---|
| `full.md` | full | the opening, the column list, every rule an expected cell depends on, and the paths | Can the agent follow a complete specification? |
| `conventions.md` | conventions | the opening, the column list, the sponsor's conventions, and the paths | Given the sponsor's vocabulary, can the agent reason out the derivation? |
| `brief.md` | brief | the opening, the column list, and the paths | How much of the specification can the agent rebuild from CDISC practice, the column names, and the data? |

Each tier states no more than the one above it: brief within
conventions, conventions within full.

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

## conventions

The full prompt with its rules cut down to the sponsor's conventions: what
the values look like, never which value a record gets. Parts 1, 2, and 4
stay verbatim; part 3 keeps only:

- **Literal values.** Category labels, flag values, fixed PARAMCD and
  PARAM, descriptive text, the values of a column that is the same on
  every record (one that never has a value included), and the set of
  values a text column takes, "has no value" among them when the full
  prompt names it.
- **Codes.** A code and the label or source value it stands for (`1 for
  <18`, `M for Male`, `"Subject refused" gives UNKNOWN`), and a catch-all
  or sentinel code as one of the values, without the condition that
  gives it (`SEX is M for Male, F for Female, or U`).
- **Units and counting.** Units and scale (days, a percentage, kg),
  conversion factors and constants, and how a duration or an age counts
  (`counting both days`, a February 29 birthday).
- **Named references.** A named method or dictionary version the values
  follow (the Mosteller value, MedDRA version 26.1), without its formula.
- **Precision.** The rounding of a value, or that it is not rounded.
- **Text form.** How a number or date is written in a text column.
- **Identifiers.** How STUDYID, USUBJID, SUBJID, SITEID, IDVARVAL, and
  similar identifiers are formed from the source.
- **Numbering.** How a sequence number or a rank runs: from where, in
  which order, within what, how ties share or break, and where a record
  with nothing to order by goes.

Everything else in part 3 is the logic under test and is dropped: which
records exist beyond the record level, which value a record gets, what a
value means, what counts as an event, censoring, precedence, ties,
windows, imputation, and what a variable holds at an edge (a zero, a
missing input, a duplicate). A sentence that mixes the two is narrowed to
its convention in the full prompt's own words (`DTHFL is Y when the
subject has a fatal adverse event ..., and has no value otherwise`
becomes `DTHFL is Y or has no value`), and it never names a value, code,
or number the full prompt does not.

A conventions prompt is written by hand from the full prompt. Where the
rules hold no convention it equals the brief prompt, and where they hold
nothing but conventions it equals the full prompt (`adam-adsl-age-group`);
either way the benchmark adds no signal at that step. Rewrite it in the
same change as its full prompt.

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
  compare. A new tier is defined in this file before any task is built
  from it.

## Checks

`python/tests/test_harbor_evaluation.py` checks that:

- every `sdtm-*` and `adam-*` benchmark has a folder here, holding
  `full.md`, `conventions.md`, and `brief.md`, and no prompt is left under
  `benchmarks/`;
- every full prompt has the four parts, and every brief prompt is exactly
  what `brief.py` writes from it;
- every conventions prompt keeps the full prompt's parts 1, 2, and 4
  verbatim, is no longer than the full prompt, and names no quoted value,
  upper-case code, or number the full prompt does not;
- every tier of a graded benchmark asks for exactly its output files and
  column lists, as the expected data holds them;
- every tier keeps to the recipe's 79-column width and never names the
  harness (yamaa, specifications, rule identifiers).

The Harbor Evaluation workflow (`.github/workflows/harbor-evaluation.yml`)
runs these checks, and every reference solution in both languages, on each
pull request that changes a benchmark or the evaluation.
