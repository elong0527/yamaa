# Benchmark prompt recipe

Canonical recipe for writing a benchmark's full prompt,
`evaluations/harbor/prompts/<name>/full.md`: the request an AI coding agent
receives when the benchmark runs as an agent evaluation (through Harbor).
The agent gets this file and the benchmark's input datasets, nothing else,
and its output datasets are graded cell by cell against the benchmark's
golden files.

- write the request a statistician would send a statistical programmer:
  the standard, the inputs, the output and its record level, every output
  column, and the outcome of each derived variable
- say what each value is, never how to compute it
- state every rule the golden depends on that CDISC practice alone does not
  settle; drop every rule for a case the inputs never hold
- write it from the benchmark's README, specification, and inputs, never
  from its golden files or anything computed from them (section 7)
- the file is committed verbatim; the evaluation copies it unchanged

## 1. Scope

- **Which benchmarks.** Every positive `sdtm-*` and `adam-*` benchmark,
  whatever its metadata lifecycle; a `draft` benchmark's prompt changes with
  it. `schema-*` benchmarks exercise the specification language and
  `negative-*` benchmarks have no dataset to deliver, so neither gets a
  prompt.
- **What is graded.** Every golden dataset the benchmark's specifications
  write, several for domains built together (DM and SUPPDM). A benchmark
  whose golden includes a run report file (`adam-adsl-age-quality`) is not
  built yet. A generated document (`define.xml`) is not graded; only
  its datasets are. A project function (`environment.yaml`) is the
  agent's to compute: state what the value is, as for any other column.
- **Where.** `evaluations/harbor/prompts/<name>/full.md`, beside the
  benchmark's other prompt tiers: `conventions.md`, written from it alone,
  keeps only the sponsor's conventions of the outcome block, and the brief
  tier is rendered from the full prompt by `brief.py` and drops the block.
  No generated brief file is committed. What each tier means is in
  `evaluations/harbor/prompts/README.md`.
- **Examples.** The examples here are invented (`XX` variables, `A` and `B`
  labels). None is a rule of a real benchmark, and no committed prompt is a
  model to copy: those were written with the golden files in view.

## 2. What the agent sees

| Sees | Never sees |
|---|---|
| the system prompt for the run's language (R or Python), then the full prompt, verbatim | `README.md`, `spec*.yaml`, `expected/` |
| `/app/input/`: the `input/` data files | benchmark metadata, yamaa itself, input schemas (`*.schema.yaml`) |
| Python and R with common data packages | the internet (web search is off) |

The agent, model, and model provider are chosen when the evaluation runs,
and the language is fixed by the task's system prompt (`system-r.md` or
`system-python.md`, which also requires `result.R` or `result.py`), so the
same benchmark prompt must work unchanged for either language. The prompt plus
the input files must be enough to reproduce every golden cell. The grader
also reruns the script from a clean state and grades what it writes.

## 3. Structure

Four blocks, in this order:

```text
Following CDISC <SDTM|ADaM> standards, use the provided <A> and <B>
datasets to create an <DOMAIN> dataset with one record per <record level>.

The output dataset should contain the following columns in this order:
<every output column, in order>

<outcome block: labels, codes, rules, precedence, and ties>

Read the source datasets from /app/input and save the completed dataset as
/app/output/<output file name>.
```

`scaffold.py` renders every block but the outcome block and the record
level from the benchmark, without its golden files: `python -m
evaluations.harbor.scaffold benchmarks/<name>` prints the skeleton, and the
prompt checks hold every full prompt to its standard, domains, column lists,
and paths.

- **Inputs.** Name each input dataset as the agent gets it, by its file
  name in upper case (`ae.csv` is AE, `xx_raw.csv` is XX_RAW).
- **Record level.** Take it from the README goal and the specification
  keys: one record per subject, per adverse event, per responder, per
  subject per parameter per visit.
- **Columns.** List every column of the specification's `output.columns`,
  pass-through columns included, in that order: the golden header's order.
- **Output file.** Use the file name of the specification's `output.path`
  under `/app/output/`. A benchmark with several output datasets
  (`spec_<domain>.yaml`) names each dataset with its own column list and
  file, in the order its entries write them.
- **Paragraphs.** Keep each block its own paragraph, the column lists of
  several datasets together in the second. The brief tier keeps the first,
  second, and last paragraphs verbatim, and `brief.py` rejects a prompt
  whose blocks it cannot find.

## 4. The outcome block

Write it from the README's `Variables:` list and `Note:`, and take each rule
they leave open from the specification, stated as an outcome. The golden
files are what the agent is graded on, never a source (section 7).

State:

- the exact literal values the output holds: category labels, flag values
  (`Y`), fixed `PARAMCD` and `PARAM`, and descriptive text, spelled exactly
  as the specification or the inputs spell them, down to British or
  American spelling;
- the mapping of a numeric code to its label (`1 for "A", 2 for "B"`);
- every key column, including how a sequence number runs (per subject or
  across the study, in which order, from which start): rows are matched
  by key, so a key numbered another way loses the whole row;
- the literal values of any text column the output fills from a lookup or
  a fixed label, such as visit names;
- a conversion factor or constant exactly as the specification states it,
  every digit included, never "the standard factor";
- the precision of a rounded result, and that a result is not rounded
  when the output keeps full precision (`XXVAL is not rounded`);
- the text form of a number or datetime the output holds in a text column,
  which is compared exactly: how many decimals, whether a whole number
  shows a decimal point, how a date and a time are joined;
- study-specific rules a CDISC-literate programmer cannot infer: what
  counts as the event, when and where a record is censored, which source
  wins when several could supply a value, and how ties break;
- what a derived variable holds when its inputs do not support a value,
  but only where the specification fixes an outcome the labels do not imply
  (`XXGR1N has no value when XXGR1 is "B"`);
- the meaning of any term a rule depends on when a programmer could read
  it two ways (a qualifying record; a complete visit).

Do not:

- mention yamaa, YAML, specifications, rule IDs, or operation names, or use
  the words *handler*, *verification*, *derivation*, or *schema* (*derive*
  as a verb is fine);
- give an algorithm, pseudo-code, join, or code;
- describe pass-through columns or ask for them to be carried or copied:
  the column list already asks for them;
- add a generic missing-value, imputation, or traceability footer;
- describe how processing fails or stops ("stops the run", "is an error",
  "no output is written"), or the order of internal steps ("once every
  row exists", "calculated a second way"): the golden is a completed
  dataset, and only its values count;
- mention sample data: subject identifiers, row counts, or particular
  dates, unless a rule names the records it applies to;
- name what only the specification uses: an intermediate result, a column
  the output does not hold;
- name a programming language, package, agent, model, or provider, or
  describe the sandbox;
- prescribe file formatting, types, or sort order, or explain
  floating-point summation order. The grader matches rows by key and reads
  an empty cell, `NA`, or `.` as no value. In number and date columns it
  compares numbers as numbers within a relative 1e-9 (`1.0` equals `1`) and
  reads dates as `YYYY-MM-DD` (a midnight datetime too). Text columns are
  compared exactly, surrounding spaces included.

Write "has no value" for a value the output leaves empty.

## 5. Fairness review

Before committing, review the prompt as the agent would read it:

1. **Cover every output column.** Trace each output column to the README
   or the specification, and each sentence of the outcome block to its
   source (section 7). Every value must follow from the prompt, the
   inputs, and ordinary CDISC practice. A rule the specification applies
   and the prompt leaves unstated needs a sentence.
2. **Rule out a second reading.** If another plausible interpretation
   gives the inputs another value, say which one applies: when "the last
   assessment before the cut-off" could take or leave an assessment on the
   cut-off day and the inputs hold one, say which.
3. **Check the keys.** Number every key column the way the prompt says
   and confirm it follows the specification's keys and numbering.
4. **Remove what no input reaches.** Drop a rule for a case the inputs
   never hold.
5. **Agree with the README.** Every `Variables:` bullet and the `Note:`
   show up in the prompt, and nothing contradicts them.
6. **Keep it short.** Plain text with simple lists, straight quotes around
   literal values, prose wrapped at 79 columns, under about 40 lines.
7. **Name the sponsor's conventions, not the standard's.** State every
   choice a CDISC programmer could reasonably make another way, such as
   how STUDYID and USUBJID come from the ODM study OID and subject key
   (as given, or joined with a hyphen). Leave out what the standard
   already fixes, such as the study day rule (day 1 is the reference
   date, no day 0): a reader who knows CDISC applies it unprompted.
8. **Let solvers try it.** Solvers that see only the full prompt and the
   inputs, with only the packages the track's system prompt
   (`evaluations/harbor/system-*.md`) lists, write the R and Python
   solutions (section 7). A passing solution, reviewed against the
   pilots' style, becomes `evaluations/harbor/solutions/<benchmark>/result.R`
   or `result.py`. Harbor's oracle agent then runs it, so the task page
   shows a readable reference instead of a script that writes the golden
   files.

## 6. Keep it in step

Change `full.md` in the same change as any edit to that benchmark's
README, inputs, or golden files, and rewrite its `conventions.md` to match.
The brief tier follows the full prompt automatically.

## 7. Writing without the golden

A prompt written with the golden files in view can carry their values, and
a solution written by the same hand can lean on what the prompt left out.
So the writer works where the benchmark's expected data does not exist,
with this recipe as its instruction, and solvers that hold only the prompt
and the inputs, as an evaluated agent does, try the prompt.
`evaluations/harbor/author.py` prepares both, as Harbor tasks or as plain
folders for agents without a sandbox.

### The writing task

Paths are relative to the task folder: `/app` in a Harbor container, the
workspace folder otherwise.

- **Sources.** `sources/` holds the benchmark's `README.md`;
  `specification/`, its specifications and input schemas without their
  verifications; `functions/`, the project functions it calls, without
  their tests; the language rules in `rules/`; `skeleton.md`; and
  `check.py` with `check.json`. `input/` holds the input datasets exactly
  as the agent gets them. Nothing else is there: no expected data, no
  engine, no earlier prompt or solution, and nothing a solver did.
- **Prompt.** Copy `sources/skeleton.md` to `output/full.md`, replace each
  `<record level>` (it may wrap across lines) and `<rules>` by sections 3
  and 4, and keep the rest verbatim.
- **Trace.** Save `output/trace.json`, a list with an entry for each
  sentence of the outcome block, quoting it and citing its sources:

  ```json
  [{"prompt": "XXFL is Y when the subject has a qualifying record",
    "sources": ["README: \"XXFL is Y for a subject with a qualifying record\"",
                "spec.yaml: columns.XXFL.derivation"]}]
  ```

  A source is `README: "<quote>"`; a specification file and a dotted path
  through mapping keys and the name or id of list entries
  (`spec.yaml: intermediates.XXEV.filter`); an input file and what it holds
  (`input/xx.csv: XXOUT`); or a rule file.
- **Questions.** When the sources cannot explain a value, say so in
  `output/questions.md` instead of guessing.
- **Check.** Before finishing, run the verifier's own check, which needs
  no golden file, and fix every problem it reports:

  ```bash
  python3 sources/check.py full output/full.md \
      --data sources/check.json --trace output/trace.json
  ```

### Running writers and solvers

Run from the repository root, prefixing each command with
`uv run --project python --no-sync`.

With Docker, Harbor runs each agent in a container that holds nothing else
(`--out` defaults to `~/.cache/yamaa-harbor/authoring`):

```bash
python -m evaluations.harbor.author write --model <provider/model> \
    --benchmarks <name> ... --out <round>
python -m evaluations.harbor.author run -c <round>/write-full/configs/<job>.json
python -m evaluations.harbor.author collect <round>/jobs/<job> --into <prompts>
python -m evaluations.harbor.author solve --prompts <prompts> \
    --model <provider/model> ... --n-attempts 3 --out <round>
python -m evaluations.harbor.author run -c <round>/solve/configs/<job>.json
python -m evaluations.harbor.author report <round>/jobs/<job>
```

Without Docker, a session hands the same folders to subagents:

```bash
python -m evaluations.harbor.author prepare --benchmarks <name> ... \
    --out <writers>
python -m evaluations.harbor.author prepare --solvers --prompts <prompts> \
    --language python r --out <solvers>
python -m evaluations.harbor.author grade <solvers>
```

`prepare` writes `<writers>/<name>-write-full/` with `instruction.md`,
`sources/`, `input/`, and an empty `output/`: what a writing task's
container holds. A candidate whose check passes is copied to
`<prompts>/<name>/full.md`. `prepare --solvers` writes
`<solvers>/<name>-candidate-<language>/` with only the evaluated agent's
`instruction.md`, `input/`, and an empty `output/`, standing for `/app`.
`grade` grades each solver's `output/` against the golden files and prints
the same report as `report`. The session that runs them:

- gives each subagent a prompt that names its folder and nothing else:
  "Work in `<folder>`. Read `instruction.md` there and follow it. Read and
  write only inside that folder.";
- never passes a subagent README text, specification content, expected
  values, an earlier candidate, or a report;
- starts a fresh subagent for every folder and attempt;
- confines each subagent to its folder where the agent tool allows,
  because without a container the folder is the only boundary, and prefers
  Harbor when Docker is available.

### After the writing task

- **Solvers.** A candidate's solver task is graded like an evaluation task
  but without a reference: the datasets, and the clean rerun under Harbor.
  No leaderboard accepts it. Use at least two models.
- **Nothing flows back.** `report` and `grade` tell the person reviewing
  the candidates which output columns the solvers failed and how often,
  never a value, key, or row count. No writer sees it, an earlier
  candidate, or anything a solver did. A failing candidate is not edited
  toward the golden: the person decides whether the README or
  specification misses a rule, a benchmark finding fixed there, and a fresh
  writing task writes from the corrected sources. Keep retries on unchanged
  sources few, and count them in the pull request.
- **Commit.** Review each passing candidate against sections 4 and 5, take
  a passing solver's program per language as its reference, and write
  the conventions prompt the same way (`--tier conventions --prompts
  <prompts>`). Name the jobs or workspaces in the pull request.

## Checks to run before finishing

Run from the repository root:

```bash
uv run --project python --no-sync pytest evaluations/harbor/tests/test_harbor_evaluation.py \
	-k "prompt or reference_solution"
uv run --project python --no-sync pytest evaluations/harbor/tests/test_harbor_authoring.py
```

The prompt tests check every tier of every benchmark: the four blocks of
the full prompt, the brief prompt against `brief.py`, the conventions
prompt against the full prompt it narrows, the column lists and output
paths against the graded datasets (the evaluation's own build, so a
benchmark it does not build yet, `adam-adsl-age-quality`, is skipped), the
full prompt's fixed parts against `scaffold.py`, the 79-column width, the
words a prompt never uses, and the fixture identifiers and dates and
specification-only names it never says. The authoring tests check that the
writer's sources are the same without `expected/`. The reference test
grades every solution in `evaluations/harbor/solutions/` against the
benchmark's current data; an R one is skipped when R lacks a package its
script loads. The Harbor workflow runs the same tests with R
installed every six hours on main and on manual dispatch; there a missing
R package fails instead.
