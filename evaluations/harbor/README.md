# Agent evaluation with Harbor

Runs yamaa benchmarks as agent evaluations: an AI coding agent gets a
benchmark's prompt and its input datasets, writes the requested dataset,
and is graded cell by cell against the benchmark's golden file.
[Harbor](https://github.com/harbor-framework/harbor) runs the agent in
Docker; this folder only writes Harbor task directories and a job file.

| File | Role |
|---|---|
| `Dockerfile` | the base image: Python and R with data packages, OpenCode's offline settings |
| `system-r.md`, `system-python.md` | the shared system prompt per language: use only that language and write `result.R`/`result.py` |
| `prompts/` | the prompts, `<benchmark>/full.md`, `conventions.md`, and `brief.md`, one file per tier; what each tier means is in [`prompts/README.md`](prompts/README.md) |
| `brief.py` | writes each `brief.md` from its `full.md` |
| `build.py` | benchmarks with a prompt -> Harbor tasks per prompt tier and language with their Harbor Hub READMEs, one dataset README per tier and language, and one job file per tier, language, and model variant |
| `grade.py` | the verifier, copied into every task's `tests/` |
| `solutions/` | reference solutions, `<benchmark>/result.R` and `result.py`, written from the full prompt alone; the oracle runs them |
| `leaderboard.py` | Harbor job directories -> Harbor Hub leaderboard and row configs |
| `leaderboards/` | leaderboard definitions, one file per leaderboard (one per prompt tier and language) |

Results are not kept in this repository: runs are uploaded to Harbor Hub and
reviewed on its leaderboards.

Tasks are built from the full prompt unless `--prompt` names the
conventions or brief tier (see [Prompt tiers](#prompt-tiers)). The Harbor
Evaluation workflow
(`.github/workflows/harbor-evaluation.yml`) reruns every reference
solution, in R and in Python, against its benchmark's current data, and
checks that every prompt still asks for the datasets and columns the
expected data holds, so a change to a benchmark that breaks either fails
its pull request.

How to write a prompt is in
[`automation/benchmark_prompt.md`](../../automation/benchmark_prompt.md).

## What a run enforces

- **One language per task.** Each task's `instruction.md` is the shared
  system prompt for its language (`system-r.md` or `system-python.md`)
  followed by the benchmark's prompt of the task's tier, which itself
  never names a language. The R track requires `/app/output/result.R`,
  the Python track `/app/output/result.py`, each rerunnable to reproduce
  the datasets. A
  shell call that runs the other language zeroes the trial: `python`,
  `pip`, or `uv` on the R track, `R` or `Rscript` on the Python track, as a
  command (also behind `env`, `sudo`, `timeout`, `bash -c`, or `$(...)`).
  Naming one is not a call: in a `grep` pattern, a quoted string, a
  comment, a heredoc body, or the tool call's description. So is a bridge
  in the script: `library(reticulate)` or `system("python ...")` in R,
  `import rpy2` or `subprocess.run(["Rscript", ...])` in Python.
- **The agent sees only the prompt and `/app/input/`.** Specifications,
  READMEs, golden files, and input schemas (`*.schema.yaml`) never enter
  its container; the golden files live in `tests/`, which Harbor builds
  into a separate verifier image.
- **Closed book.** While the agent works, only the model API host is
  reachable. During setup Harbor may also reach GitHub, `nodejs.org`, and
  the npm registry to install OpenCode. OpenCode's `webfetch` and
  `websearch` tools are denied, and the grader zeroes any trial whose
  trajectory calls them. The verifier has no network.
- **Grading.** Reward 1 when every requested dataset has the golden's
  columns, keys, and cell values, and the script, rerun by the verifier
  from a clean state (its datasets deleted first), writes datasets that
  match too. Then the held-out rerun: with every third subject (by
  `USUBJID`) dropped from the inputs, the script must write the golden
  restricted to the subjects kept, so a script that writes its rows
  literally fails. Every task ships its reference solution (`solutions/`,
  copied into the verifier's `tests/reference/`), and the check applies
  when that reference, rerun the same way, writes exactly the restricted
  golden; it is skipped with a note when the reference's data differs (a
  derivation across subjects, or inputs without `USUBJID`), and a
  reference that crashes is a verifier error, never an exemption. Then,
  when an output carries `USUBJID`, the changed-input challenge: every
  subject identifier in the inputs is renamed, the reference computes the
  answer from those inputs, and the script must write it, so a lookup of
  the expected rows filtered to the subjects present fails. Column and row
  order are reported, not graded. Cells are read by column type: integers
  compare exactly (`1.0` equals `1`), other numbers within a relative 1e-9,
  dates also accept a midnight datetime, text is compared exactly (spaces
  included), and an empty cell, `NA`, or `.` is no value. An output that is a symlink
  is rejected.
  `reward.json` also carries `cell_accuracy`, `row_accuracy`, `reproduced`,
  `language_violations`, `held_out_checked`, `held_out` (when checked),
  `challenge_checked`, and `challenge_passed`, and
  `verifier/diff-<file>.csv` lists the differing cells.
- **Sandboxed reruns.** The verifier runs the agent's script, and the
  reference, as the image's unprivileged `nobody` user (`sandbox.py`):
  `/tests`, with the golden and the reference, is readable by root only,
  every rerun restores the original inputs from `tests/input/` and starts
  from an empty output folder, and every process the script left is
  killed. A model's job also requires the agent's trajectory
  (`YAMAA_REQUIRE_TRAJECTORY`), so the language and web-tool audit always
  has a record to read. The verifier saves the task's `task.toml` with its
  results, so a later build cannot change the commit a trial reports.

## Setup

A Docker engine whose kernel supports nftables `fib` (Harbor's allowlist
needs it): Linux, or OrbStack on macOS. Docker Desktop may lack it.

```bash
uv sync --project python --group harbor
docker build -t yamaa-harbor-env:0.3 evaluations/harbor
```

## Build and check the tasks

`build.py` writes to `~/.cache/yamaa-harbor/` (`$XDG_CACHE_HOME/yamaa-harbor`
when set, or `--out`): the tasks under `tasks/`, one dataset README per
language under `datasets/<language>/`, and the job files under `configs/`.
Harbor writes its job directories under `jobs/` there. All of it stays
outside the repository, whose validators read every file in the tree. Each
build first removes the tasks, datasets, and job files of the previous one.

Each language is its own Harbor Hub dataset, `yamaa/yamaa-sdtm-adam-r` and
`yamaa/yamaa-sdtm-adam-python` (`--dataset-prefix` changes the part before
`-<language>`), so R and Python are published, run, and ranked apart. Each
task gets a `README.md`, its page on Harbor Hub: the benchmark's own
README, the inputs, the outputs and keys, and the grading and network
rules. It sits at the task root, which never enters the agent's container.
`datasets/<language>/README.md` is the dataset's page, listing its tasks.

```bash
uv run --project python --no-sync python evaluations/harbor/build.py \
	--model opencode-go/muse-spark-1.3-contributor
```

One build covers both tracks (`--language r python`, the default); pass
`--language r` or `--language python` to build a single track. Each
benchmark becomes `<benchmark>-r` and `<benchmark>-python` tasks.
A benchmark is skipped, with its reason on stderr, when the grader
cannot check it: its golden is a warning or violation log, a golden cell
holds a token the grader reads as no value (`NA`, `.`), a column's type is
not one the grader knows, or no specification writes a golden dataset.
Naming one with `--benchmarks` fails fast instead. Domains built together
(`spec_dm.yaml` and `spec_suppdm.yaml`) are one task that grades both
datasets.

### Prompt tiers

`--prompt` picks the prompt tiers to build: `full` (the default),
`conventions`, and `brief`; [`prompts/README.md`](prompts/README.md) says
what each keeps. Every tier is its own Hub dataset per language,
`yamaa/yamaa-sdtm-adam-<tier>-<language>`, with tasks
`<benchmark>-<tier>-<language>`, its page in `datasets/<tier>-<language>/`,
and job files `<job-name>-<tier>-<language>[-<variant>].json`; the full
prompt keeps the names above. Inputs, expected data, the grader, and the
reference solution are the same in every tier, so the oracle scores 1 on
every task and `nop` 0. On a shorter prompt that shows the task builds and
grades, not that the prompt can be solved, and each tier task's page says
so.

```bash
uv run --project python --no-sync python evaluations/harbor/build.py \
	--prompt conventions brief \
	--model opencode-go/muse-spark-1.3-contributor \
	--variant low \
	--out ~/.cache/yamaa-harbor-tiers
```

To compare tiers, run the same model and variant, with the same attempts,
on all three, and read its rows on the three boards of one language.

For development, a pilot of three ADaM benchmarks in both languages is a
quick check before a full run:

```bash
uv run --project python --no-sync python evaluations/harbor/build.py \
	--benchmarks adam-adsl-age-group adam-adae-death adam-adtte-dor \
	--model opencode-go/muse-spark-1.3-contributor \
	--n-concurrent 3 \
	--job-name muse-spark-1.3-pilot
```

Harbor's `oracle` agent runs each task's `solution/result.R` or
`result.py` and must score 1 on every task, rerun included; its `nop`
agent writes nothing and must score 0. That script is the benchmark's
reference solution from `solutions/`, written from the full prompt and
the inputs alone, which also shows the prompt can be solved. Every task
needs one, since the held-out and changed-input reruns compute their
answers with it, and a benchmark without one is not built. To run the
oracle:

```bash
uv run --project python --no-sync harbor run \
	-p ~/.cache/yamaa-harbor/tasks \
	-a oracle \
	-o ~/.cache/yamaa-harbor/jobs \
	--job-name oracle \
	-y
```

`smoke.py` does both checks offline in one step: it builds the tasks,
probes the verifier sandbox (`probe.py`: the script cannot read the golden
files, change the inputs or the grader's logs, or leave a process
behind), then runs `oracle`, which must score 1 with every required
challenge checked, and `nop`, which must score 0. The Harbor Oracle
workflow (`.github/workflows/harbor-oracle.yml`) runs it on the three
development tasks for each pull request, and on every benchmark on
Mondays or on request; before a model run, run it on every benchmark:

```bash
uv run --project python --no-sync python evaluations/harbor/smoke.py \
	--full \
	--n-concurrent 8 \
	--out ~/.cache/yamaa-harbor-smoke
```

## Run an agent

The provider is chosen per run: `--model <provider>/<model>` picks the
model API host and the key variable, and the key is read from your shell
when the job starts, never written to a file.

| Provider prefix | Host | Key |
|---|---|---|
| `opencode-go`, `opencode` | `opencode.ai` | `OPENCODE_API_KEY` |
| `anthropic` | `api.anthropic.com` | `ANTHROPIC_API_KEY` |
| `openai` | `api.openai.com` | `OPENAI_API_KEY` |
| `xai` | `api.x.ai` | `XAI_API_KEY` |

Any other provider, or a gateway, takes `--api-host` and `--key-env`.
Each job file allows the agent 900 s to install (`override_setup_timeout_sec`),
since many trials downloading at once slow it; the agent's own time limit
is the task's, which leaderboard runs must keep.

The build writes one job file per language and model variant,
`configs/<job-name>-<language>-<variant>.json`, so each job is one row on
one language's leaderboard. `--variant` takes the model's OpenCode
variants, which set its reasoning effort; for
`opencode-go/muse-spark-1.3-contributor` they are `minimal`, `low`,
`medium`, `high`, and `xhigh` (`opencode models opencode-go --verbose`
lists them). Without `--variant` there is one job per language at the
model's default. `--job-name` is the prefix, by default the model's name.

```bash
export OPENCODE_API_KEY=...
for job in ~/.cache/yamaa-harbor/configs/*.json; do
	uv run --project python --no-sync harbor run -c "$job" -y
done
```

Each trial directory under `~/.cache/yamaa-harbor/jobs/<job>/` holds the
agent's files (`artifacts/app/`, with its `output/result.R` or
`output/result.py`) and trajectory (`agent/trajectory.json`),
the grade (`verifier/grade.json`, `verifier/reward.json`, and a
`diff-<file>.csv` when cells differ), and Harbor's `result.json` with
tokens and cost. `harbor view ~/.cache/yamaa-harbor/jobs` browses them.

## Leaderboards

Each leaderboard follows the
[Harbor Hub model](https://docs.harborframework.com/core-concepts/harbor-hub/leaderboards):
a curated, ranked table whose definition fixes the columns and the ranking
rules, and whose rows are uploaded runs that point back to their trials.
`leaderboards/<name>.yaml` holds:

- `package`: the Harbor Hub dataset the board belongs to, the dataset of
  its language and prompt tier.
- `grading_protocol`: the verifier's checks a run must have been graded
  with (`GRADING_PROTOCOL` in `build.py`, recorded in every task).
- `tasks` and `attempts`: the benchmarks a run must cover and the fewest
  attempts on each (every buildable benchmark, one attempt), with the
  same number of attempts on every task.
- `harbor`: the Hub definition itself, in the shape
  `harbor hub leaderboard create --config` takes: the `metadata_schema`
  and `metrics_schema` of a row, the `columns`, and the ordered `rank_by`
  rules. Accessors read one flat key, `metadata.<key>` or `metrics.<key>`.

A leaderboard's tasks are its fixed question, as a Hub leaderboard is
pinned to dataset versions: adding a task leaves earlier runs without it,
and `export` then refuses them. Start a new leaderboard instead. There is
one leaderboard per dataset, so one per language and prompt tier
(`yamaa/yamaa-sdtm-adam-r/sdtm-adam-v2-r`,
`yamaa/yamaa-sdtm-adam-brief-r/sdtm-adam-v2-brief-r`, ...), and R, Python,
and each tier are ranked independently. The conventions and brief boards
rank by cell accuracy before pass rate, since their tasks can need a
sponsor choice the prompt no longer states. The boards are `v2` because
grading protocol 2 adds the changed-input challenge, the sandboxed reruns,
and the trajectory requirement: runs graded before it stay on the earlier
`sdtm-adam-r` and `sdtm-adam-python` boards on Harbor Hub, whose
definitions this folder no longer keeps.

A row is one job: one agent, model, and variant, so the variants of a model
sit side by side. Its metadata comes from the job (agent, version, model,
variant, date, attempts, job and Harbor version, and the yamaa commit,
grading protocol, and image of its tasks, read from the `task.toml` each
trial's verifier saved) and its metrics from its trials on the
leaderboard's tasks:

| Metric | Meaning |
|---|---|
| `reward` | the pass rate: each task's mean reward, averaged over tasks; an errored trial counts as 0 |
| `trial_reward` | the mean reward over trials (Harbor's mean) |
| `reward_ci_low`, `reward_ci_high` | a 95% bootstrap interval of `reward` over tasks (2,000 resamples); it shows how much the score depends on which tasks are in, not the noise within a task |
| `cell_accuracy`, `row_accuracy` | mean share of golden cells and rows reproduced |
| `pass_at_<k>` | Harbor's unbiased pass@k averaged over tasks, for k = 2, 4, 5, 8, 10, ... up to the fewest attempts on a task |
| `n_trials`, `n_errors` | trials aggregated, and trials that ended in an exception |
| `input_tokens`, `output_tokens`, `cost_usd` | totals, when every trial reported them |
| `cost_per_trial_usd` | `cost_usd` per trial |
| `agent_seconds` | mean agent time per trial |

Harbor skips pass@k when a verifier writes several rewards, as `grade.py`
does, so `leaderboard.py` computes it from `reward`. Harbor Hub orders the
rows by the board's `rank_by` rules. With the `harbor` group installed,
tests check the pass@k estimator and the exported configs against
Harbor's own code; without it they skip.

To compare models, ask for several attempts per task: one attempt is a
pilot, and pass@k needs at least two. Terminal-Bench 2.0, the reference
Harbor benchmark, takes leaderboard runs with `--n-attempts 5`.

### Publish to Harbor Hub

Everything is private to the `yamaa` organization on Harbor Hub: the tasks,
the two datasets, the uploaded jobs, and the leaderboards. Log in once with
`harbor auth login` (GitHub OAuth) as a member of `yamaa`. Build the tasks
from a committed tree, so each row's "Tasks from" commit names the prompts
it answered (a `+dirty` suffix means uncommitted changes), and publish
exactly the task directories the job ran, so the uploaded trials match the
published tasks. Publish the tasks before the datasets, whose manifests
point at them; `dataset init` keeps the README the build wrote. Publish one
tier from a build of that tier alone, in its own `--out`: `tasks/*-r` also
matches `*-brief-r` and `*-conventions-r`. For the R dataset of the full
prompt (repeat with `python`; for a tier, use its dataset name and its
`datasets/<tier>-r` folder):

```bash
H="uv run --project python --no-sync harbor"
D=~/.cache/yamaa-harbor/datasets/r
$H publish ~/.cache/yamaa-harbor/tasks/*-r \
	--private
$H dataset init yamaa/yamaa-sdtm-adam-r \
	-o $D \
	--description "yamaa SDTM and ADaM benchmarks, R track"
$H add ~/.cache/yamaa-harbor/tasks/*-r \
	--to $D
$H publish $D \
	--private
$H upload ~/.cache/yamaa-harbor/jobs/<job> \
	--org yamaa \
	--private
```

A republish of changed tasks makes a new revision of each task and of the
dataset. The board links dataset revisions by id, and
`harbor hub leaderboard update <board> --dv-id <id> ...` replaces that
list, so pass every revision its rows ran on.

Then write each leaderboard's configs from the job directories and create
the board and its rows:

```bash
uv run --project python --no-sync python evaluations/harbor/leaderboard.py \
	export sdtm-adam-v2-r \
	~/.cache/yamaa-harbor/jobs/muse-spark-1.3-contributor-r-low
$H hub leaderboard create \
	--config ~/.cache/yamaa-harbor/hub/sdtm-adam-v2-r.leaderboard.yaml
$H hub leaderboard row create yamaa/yamaa-sdtm-adam-r/sdtm-adam-v2-r \
	--config ~/.cache/yamaa-harbor/hub/sdtm-adam-v2-r.rows.yaml
```

`export` takes the board's dataset from its `package`; `--package`
overrides it.

An uploaded job keeps each trial's artifacts, so the agent's script stays
on the Hub under the trial's `artifacts/app/output/`, and
`harbor job download <job-id>` brings a whole job back, scripts included.

`export` refuses a job that mixes agents, models, or variants, misses a
board task or attempts, has unequal attempts across tasks, or changed the
tasks' timeouts, resources, mounts, network access, skills, or verifier.
It also refuses a trial without the grading evidence `grade.json` keeps
(a checked trajectory, and the challenge where one is required), and a
job whose tasks do not share one committed (not `+dirty`) yamaa commit,
the board's grading protocol, and one image. `--hide` exports its rows
hidden. Repeat for `sdtm-adam-v2-python` with the Python jobs. The
published tasks
build from the local `yamaa-harbor-env:0.3` image, so they run where that
image is built.
