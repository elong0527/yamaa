# opencode agentic evaluation

Turns every benchmark under [`benchmarks/`](../../benchmarks/) into one
agentic task: a realistic prompt plus a starting workspace. The task runs
through [opencode](https://opencode.ai) with a chosen model, and the finished
workspace is judged by the conformance runner's own comparator
(`yamaa.adapters.conformance.compare_example`). The headline number is how
many cases **exactly** match their committed answer.

```bash
# from the repository root; a task set and a run directory live outside it
uv sync --project python
E="uv run --project python python automation/opencode-eval/opencode_eval.py"

$E generate  --tasks /tmp/yamaa-eval/tasks            # 312 prompts + workspaces
$E verify    --tasks /tmp/yamaa-eval/tasks            # coverage is exactly one to one
$E calibrate --tasks /tmp/yamaa-eval/tasks --run-dir /tmp/yamaa-eval/calibrate
$E run       --tasks /tmp/yamaa-eval/tasks --run-dir /tmp/yamaa-eval/sonnet \
             --model anthropic/claude-sonnet-5-5 --jobs 4
$E grade     --tasks /tmp/yamaa-eval/tasks --run-dir /tmp/yamaa-eval/sonnet  # re-judge
```

`run` needs `opencode` on `PATH` (`npm i -g opencode-ai`) and the provider's
credentials in the environment (for example `ANTHROPIC_API_KEY`). Pass
benchmark names after the flags to run a subset.

## 1. Assessment of the benchmark

| | Count |
|---|---|
| Benchmarks | 312 (80 `adam-*`, 54 `sdtm-*`, 42 `schema-*`, 136 `negative-*`) |
| In `execution-manifest.yaml`, all `executable` on Python | 312 |
| Reference engine passes (`python -m yamaa.adapters.conformance`) | 312 / 312 in ~46 s |
| Positive (commit an artifact) / negative (commit `expected/error.yaml`) | 176 / 136 |
| Negative phases | 79 validation, 15 verification, 10 derivation, 9 join, 8 ingest, 5 convert, 5 impute, 2 output, 1 cut, 1 mapping, 1 row_construction |
| Artifacts the comparator judges (`expected/*.csv`, `*.parquet`) | 183 |

What makes it a good agentic benchmark:

- **The requirement already exists in prose.** Every README carries `Goal`
  and `Input`; 290 carry `Variables` and 226 a `Note`. They are written
  without YAML, so they are the natural source of a prompt.
- **The answer is exact and the judge already exists.** CSV compares byte
  for byte and parquet logically, through the same conformance suite the
  Python CI job runs.
- **Inputs are tiny**, so a run is cheap and a failure is easy to read.
- **Negatives test "fail instead of choosing"**, the behaviour that matters
  most when an agent handles regulated data.

What has to be handled before it can be used that way:

1. **The answer sits beside the question.** `spec.yaml` is the solution,
   136 READMEs end in `## How to fix`, five READMEs point at
   `expected/...`, every negative title opens with *Reject* or *Fail*, and
   105 negative READMEs say outright that the run is "rejected". The
   repository and its dashboards are public, so web access is a leak too.
2. **Negatives are mostly about the specification, not the request.** 79 of
   136 fail at validation: a misspelled window name, a type of `number`, a
   dependency cycle. An agent writing its own specification from a prompt
   would never write those defects, so a prompt-only task cannot reproduce
   them. They need the committed specification handed over.
3. **Byte-exact means yamaa-rendered.** The expected CSV is the yamaa CSV
   profile's rendering (`60.0` in, `60` out), so a pandas answer can be right
   and still differ in bytes. The task therefore asks for a yamaa
   specification, and the judge re-runs it.
4. **Some dataset metadata lives only in the spec**: keys, `output.order_by`,
   `output.decimals`, labels, and types. A programmer's spec sheet has these,
   so the prompt carries them; derivation logic stays out.
5. **README voice.** 21 of the 42 `schema-*` READMEs describe the
   specification in `Input` ("one `spec.yaml` over one lab-results file")
   rather than the data, and four READMEs say "benchmark". Those `schema-*`
   cases test language features more than requests, and their prompts read
   less like a real ticket. Either keep them as a separate "language" slice
   or give them request-voice READMEs.
6. **Six committed files are not judged**: four `spec_resolved.yaml`, and
   `sdtm-dm-metadata`'s `define.xml` and `dm.json`. The conformance comparator
   reads only `.csv` and `.parquet`, so exact match covers the other 183.

## 2. Proposal

### Two task kinds, decided the way the comparator decides them

`expected_kind()` already classifies a benchmark by whether it commits
`expected/error.yaml`. The same rule picks the task:

| Kind | Cases | The agent gets | The agent must |
|---|---|---|---|
| **author** (positive) | 176 | prompt, `input/`, project files | write the entry specification, run it, leave the artifacts in `output/` |
| **run and report** (negative) | 136 | prompt, `input/`, project files, **the committed specification** | run it, not work around the failure, record the diagnostic in `output/error.yaml` and the cause in `output/NOTES.md` |

The negative framing is how the failure happens in practice: a reviewed
specification meets data or an engine that rejects it. Its prompt never says
the run will fail.

### How a prompt is built

Deterministically, with no model in the loop, from what the repository
already holds:

| Prompt part | Source |
|---|---|
| Title | README `#` heading (negatives: "Run the ADxx specification") |
| Opening request | domain and standard, in a ticket voice |
| What I need | README `Goal` (a leading "attempt" is dropped) |
| Source data | README `Input` + a manifest of each input file's columns and record count |
| Variables | keys, `output.order_by`, `output.decimals`, then variable / label / type from the resolved spec, then README `Variables` |
| Notes | README `Note` and any other labelled section |
| Deliverables | the entry specification's file name, and every judged `expected/` file with its header, in order |

Never in a prompt or a workspace: README `## How to fix`, anything pointing
at `expected/`, `run.R`, `run.py`, `README.md`, `expected/`, and (for an
author task) the entry specification. Parents, producers, and project
routines a benchmark keeps beside its entry are handed over as given files,
because a real study has those already.

A realism layer can sit on top: a model rewrites each deterministic prompt
in a persona's voice (terse lead, verbose CRO ticket, ...). It must keep
every backticked token, variable name, file name, and number the
deterministic prompt carries; a rewrite that drops one is rejected. That
check keeps the paraphrase from dropping a rule the exact answer depends on.
It is not built yet.

### The workspace

```
task-012/                 # named by task id, never by benchmark
  input/...               # given, byte-checked afterwards
  <project files>         # given: parent specs, environment.yaml, python/...
  AGENTS.md               # standing rules: yamaa only, output/ only via the run
  opencode.json           # edit+bash allow; webfetch, websearch, external_directory, question deny
  yamaa_run.py            # runs the entry spec, publishes into output/, prints YAML
  reference/              # yaml/ schema, rules/, articles/ (not benchmark.md), python.md
  output/
```

`run` invokes, per task:

```bash
opencode run --model <provider/model> --dir <workspace> --format json \
  --title task-012 "<PROMPT.md>"
```

The run is isolated in these ways:

- **A fresh `HOME`.** Without it, opencode reads the machine's own
  configuration, and even `~/.claude/skills`.
- **yamaa installed from a wheel built into the run directory.** An editable
  install points `yamaa.__file__` back at this checkout, and a plain install
  of the source directory records the checkout path in `direct_url.json`.
- **An environment and `PATH` with nothing that names the checkout**, and
  `run` refuses a run directory inside the repository.
- **The network limited to the model provider.** opencode's permissions stop
  its own fetch tools but not `curl` from a shell, and the reference docs
  link to the public repository and dashboards. Run it in a container
  with that network policy.

Each transcript is also scanned for markers that only appear if the agent
reached the answers: the checkout path, `/expected/`, and raw or API GitHub
URLs for the repository. A hit fails the case. A bare repository URL is not
a marker, because the reference docs contain it.

### How a case is judged

A case **exactly matches** when all three hold:

1. **Integrity**: every given file is byte-identical to the one handed out,
   and a given symbolic link is still the same link.
2. **Reproduction**: the finished workspace, run afresh by the reference
   engine on the pristine inputs, satisfies `compare_example`: byte-exact CSV
   and logical parquet for a positive; phase, condition, spec paths,
   requirement, and context for a negative.
3. **Delivery**: `output/` holds exactly what that run publishes (positive),
   or the `output/error.yaml` the agent wrote itself satisfies
   `compare_example` and no dataset was published (negative).

Re-running the specification, rather than trusting `output/`, is what stops a
hand-written CSV from passing. `results.jsonl` records every finding and the
record counts reproduced against committed; `summary.json` counts exact
matches by kind.

## 3. Verification

`verify` checks that the task set covers the benchmarks one to one:

```
benchmarks: 312
tasks:      312 {'negative': 136, 'positive': 176}
judged expected artifacts: 183
committed but not judged:  6 [4 x spec_resolved.yaml, define.xml, dm.json]
verify: ok
```

- benchmark directories = tasks = `execution-manifest.yaml` entries (312);
- a task is negative exactly when `expected/error.yaml` exists, and a
  validation-phase negative exactly when `validation-manifest.yaml` lists it;
- every author task promises exactly the judged `expected/` file set, each
  with the committed header in order, and the variable table equals the
  primary artifact's header;
- every given file is the benchmark's, byte for byte, and a committed
  symbolic link stays a link (`negative-path-symlink` is rejected for being
  one);
- no workspace carries a withheld file or an author task's entry spec, and no
  prompt carries `How to fix`, `schema_version`, `derivation:`, or
  `expected/`.

`test_opencode_eval.py` runs the same checks under `unittest`:

```bash
uv run --project python python -m unittest discover -s automation/opencode-eval
```

`calibrate` proves the judge counts exact matches correctly, with four
scripted agents going through the same `yamaa_run.py` a model uses:

| Agent | Does | Exact matches |
|---|---|---|
| reference | copies the committed entry spec (positive) or copies the printed diagnostic (negative) | **312 / 312** (176 / 176 positive, 136 / 136 negative) |
| empty | nothing | 0 / 312 |
| perturbed | reference, then drops the last delivered record, or misnames the condition | 0 / 312 |
| tampered | reference, then leaves one given file changed | 0 / 312 |

Every miss fails for its own reason: an empty agent on "no entry
specification" or "error.yaml not delivered", a perturbed one on the
delivered artifact or the diagnostic's condition, and a tampered one on
integrity. For the reference agent, all 183 judged artifacts reproduce with
the committed record count. The four agents take about 10 minutes together
with `--jobs 8`.

No model run is recorded here, because the environment this was built in has
no provider credentials. What was checked: opencode 1.18.33 accepts the flags
above; `opencode debug config` resolves the workspace's `opencode.json` with
the deny rules in force; and a `run` without credentials started opencode in
the workspace, logged its JSON error event, and was graded 0/1.

## 4. Example prompts

Generated by `generate`, unedited.

### Author task: `adam-adae-severity`

```markdown
# Carry Each Event's Severity

I need the ADAE dataset (ADaM) built as a yamaa specification from the files in this workspace. Please write the specification, run it with `python yamaa_run.py`, and leave the result in `output/`.

## What I need

One Analysis Dataset for Adverse Events (ADAE) row per collected adverse event (AE), carrying `AEDECOD` and `AESEV`.

## Source data

Collected adverse events with their coded terms, plus supplemental records carrying the severity recorded for each event.

| File | Columns | Records |
|---|---|---|
| `input/ae.csv` | STUDYID, USUBJID, AESEQ, AEDECOD | 8 |
| `input/supp.csv` | STUDYID, USUBJID, AESEQ, AESEV | 6 |

## Variables

One record per STUDYID, USUBJID, AESEQ.

| Variable | Label | Type |
|---|---|---|
| `STUDYID` | Study Identifier | str |
| `USUBJID` | Unique Subject Identifier | str |
| `AESEQ` | Sequence Number | int |
| `AEDECOD` | Dictionary-Derived Term | str |
| `AESEV` | Severity/Intensity | str |

- `AEDECOD` is the dictionary-derived term collected for the event; always present from the collected record.
- `AESEV` is the severity/intensity recorded for that event; empty when the event has no supplemental record, or the record carries no severity.

## Notes

Severity is matched on the study and subject identifiers together with the event sequence number, so two events for one subject keep their own severities apart. A supplemental record for an event that was never collected is ignored: it creates no row.

## Deliverables

- `spec.yaml`: the entry specification. Any further specification it needs goes beside it as `spec_<name>.yaml`, named by the entry as a parent or a producer.
- `python yamaa_run.py` must publish exactly these files, with the columns in this order:
  - `output/adae.csv`: STUDYID, USUBJID, AESEQ, AEDECOD, AESEV
- Everything in `output/` must come from that run; do not write or edit it by hand.
```

### Run-and-report task: `negative-adeg-rrr`

The committed `spec.yaml` and `input/adeg.csv` are in the workspace. The
engine rejects the run at verification (`implication_failed`), and the task
passes only when the agent records that diagnostic and leaves both files
untouched.

```markdown
# Run the ADEG specification

`spec.yaml` in this workspace is the reviewed ADEG specification (ADaM). Please run it on the files here with `python yamaa_run.py` and deliver `output/adeg.csv`.

## What the specification is for

Keep each collected heart rate (HR) record and add a rederived RR duration (time between successive R waves) record under the code `RRR` for each subject and analysis visit with a non-missing, non-zero HR result, labeled `RR Duration Rederived (ms)` and carrying the new result in `AVAL` and its unit in `AVALU`.

## Files

| File | Columns | Records |
|---|---|---|
| `input/adeg.csv` | STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, AVALU | 6 |

Given, beside the inputs: `spec.yaml`.

## Ground rules

- The specification, `input/`, and the other given files are under change control: do not edit, rename, or delete them.
- If the run cannot produce the dataset, do not work around it. Record the diagnostic in `output/error.yaml` as AGENTS.md describes, and put the cause and the change you would propose in `output/NOTES.md`.
```

## 5. Next steps

- Run each task *k* times per model and report pass@1 and pass^k; opencode's
  JSON events carry the token counts for cost.
- For the 57 negatives that pass validation and fail later, mostly on a
  defect in the data, an author variant is possible: the agent writes its own
  spec from the README and is judged on phase and condition only, since spec
  paths depend on the spec it wrote.
- Grade `output/NOTES.md` against the withheld `## How to fix` with a rubric
  judge, as a secondary score.
- Build the persona paraphrase layer with its token-preservation check.
