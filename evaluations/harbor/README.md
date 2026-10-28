# Agent evaluation with Harbor

An agent receives a benchmark's prompt and input datasets, writes an R,
Python or opensas program, and reproduces the independently authored
expected datasets. [Harbor](https://github.com/harbor-framework/harbor)
runs the agent and verifier in separate Docker images. Evaluation results
live on Harbor Hub or in the chosen output directory.

## Sources and generated output

| Source | Purpose |
|---|---|
| `prompts/<benchmark>/full.md` and `conventions.md` | authored requests; the brief tier is derived from the full prompt when used |
| `solutions/<benchmark>/result.R`, `result.py`, `result.sas` | independent reference programs written from the prompt and inputs |
| `scaffold.py`, `check.py` | the full prompt's parts the benchmark fixes, and the prompt checks shared by writers, verifiers and tests |
| `packet.py`, `author.py` | prompt writing without the expected data: the writer's sources, writing and candidate solver tasks or plain folders, and a redacted solver report |
| `system-*.md` | language policy and packages available to the agent |
| `Dockerfile`, `install-r-packages.R`, `images.py` | language images and their runtime checks |
| `build.py` | tasks, dataset pages, and model job configurations |
| `grade.py`, `sandbox.py` | grading and isolated program reruns |
| `measurements.json` | authored scaling columns and text substitutions for changed-input checks |
| `leaderboards/config.yaml` | shared Hub schemas, columns, ranking and minimum attempts |
| `leaderboards/task-populations.json` | frozen benchmark populations for versioned boards |
| `leaderboard.py`, `hub.py`, `run.py` | result aggregation and private Hub publication |
| `tests/` | prompt, reference, grading, isolation and Hub controls |
| `pyproject.toml`, `uv.lock` | the evaluation's own environment, locked apart from the package: Harbor, the engine built from the checkout and the repository tools |

Benchmark lifecycle, standard, domain and track exclusions come from each
case's `benchmarks/<name>/benchmark.yaml`. Adding a benchmark needs its
metadata, authored prompts and independent reference programs; it needs no
committed brief prompt or per-language leaderboard definition. Follow the
[prompt recipe](prompts/recipe.md) and [tier contract](prompts/README.md).
Prompts are written without the expected data: `author.py` runs the writer
in a Harbor sandbox that holds only the benchmark's sources, and tries each
candidate with solvers that see only the prompt and inputs (recipe section
7); without Docker, `author.py prepare` builds the same folders for
subagents. Nothing the solvers do reaches a writer, and those jobs run
locally and are never uploaded.
The [opensas guide](solutions/opensas.md) covers its runtime and supported
inputs. An explicit request for an excluded track fails with the reason
from that case's metadata.

Tasks, jobs, image diagnostics and exported Hub configurations are generated
outside the checkout. The default is `~/.cache/yamaa-harbor/`, or
`$XDG_CACHE_HOME/yamaa-harbor`; commands accepting `--out` can override it.
A build replaces the tasks, dataset pages and job configurations in that
output directory, while preserving completed jobs.

## Setup and local checks

Use a Docker engine with nftables `fib` support for Harbor's network
allowlist, such as Linux or OrbStack. Install the locked evaluation
environment from the repository root:

```bash
uv sync --project evaluations/harbor
uv run --project evaluations/harbor --no-sync python -m evaluations.harbor.images build
uv run --project evaluations/harbor --no-sync python -m evaluations.harbor.images verify
uv run --project evaluations/harbor --no-sync pytest evaluations/harbor/tests
```

Images are separate agent/verifier pairs for R, Python and opensas. Each
agent image exposes its required runtime, OpenCode and shell text tools.
The verifier adds the trusted Python grader. R packages are installed and
checked by `install-r-packages.R`, shared by the image and CI. Local
reference tests skip unavailable optional runtimes; CI requires its R
packages and runs opensas oracle checks in the built image.

`images verify` checks approved packages, foreign-runtime isolation and
OpenCode permissions before and after Harbor's pinned installer. A local
provider tests both allowed and denied calls without a model credential.
The `Harbor` workflow runs on manual dispatch.
Its `solutions` job checks references and prompts; its `oracle` job runs the
harness controls, image checks and real oracle/nop trials; its `advisories`
job checks `uv.lock` with pip-audit and OSV-Scanner.

Run an offline Docker preflight before spending model tokens:

```bash
uv run --project evaluations/harbor --no-sync python -m evaluations.harbor.smoke \
    --out /tmp/yamaa-harbor-preflight
```

The default samples buildable tasks and saves the selection and seed.
Use `--language opensas --all-cases` for its full supported population,
`--benchmarks <name> ...` for specific tasks, and `--seed` to repeat a
sample. Oracle must pass and nop must fail through Harbor's actual
verifier, with no model API calls.

## Build and run a model

```bash
uv run --project evaluations/harbor --no-sync python -m evaluations.harbor.build \
    --model openai/<model> --language python --prompt full \
    --out /tmp/yamaa-harbor-model
uv run --project evaluations/harbor --no-sync python -m evaluations.harbor.run \
    -c /tmp/yamaa-harbor-model/configs/<generated-job>.json
```

`build --help` lists model variants, provider overrides, attempts and
concurrency. Defaults build R and Python full-prompt tasks for every
prompted benchmark. `--prompt conventions brief` selects the other tiers;
each language and tier has a separate dataset and job. Credentials come
from the provider's named environment variable and stay out of task files.

`run` preserves each task and `evaluation.json` before execution. A model
job requires a trajectory, uploads its completed tasks, datasets, job and
eligible leaderboard rows privately to the `yamaa` organization, verifies
the associations, then cleans uploaded local artifacts. Log in with
`harbor auth login` before a publication run. Use the generated jobs from
a committed tree so the stored source revision names the prompts answered.
Restore a cleaned job with `harbor job download <job-id>`.

## Grading contract

The agent sees its system prompt, task prompt and `/app/input/` data.
Specifications, input schemas, benchmark READMEs, expected data and
references stay out of the agent image. Only the model API host is
reachable while the agent works; the verifier has no network.

Each track requires its own program (`result.R`, `result.py` or
`result.sas`). Calls to another runtime, helper compiler or web tool fail
the language policy, including denied attempts, shell wrappers and bridges
inside a program. Merely naming a runtime in text is allowed. OpenCode
permissions provide an early guard; the independent trajectory audit
makes the scoring decision.

A passing result must match every requested dataset and reproduce it from
a clean output directory. The verifier also reruns both the reference and
agent program after dropping selected subjects and after changing supported
identifiers, ages, measurements and complete dates. `measurements.json`
adds benchmark-specific numeric measurements; categorical codes and counts
are not measurements. Partial dates remain unchanged. These checks exercise
supported counterfactuals and do not prove generalization to every input.

Reruns execute as an unprivileged user with no access to expected files or
references. Inputs are restored between runs, outputs are removed, and
remaining child processes are killed. A reference must first reproduce the
original expected data. Its recomputed answer supplies expectations for
cross-subject derivations; subject-local results use restricted expected
data when the reference agrees. A reference failure is a verifier error.

Integer cells compare exactly, other numbers within relative `1e-9`, and
text including spaces exactly. Empty cells, `NA` and `.` mean no value;
dates also accept midnight datetimes. Row and column order are reported
without affecting the score. Extra rows or columns reduce partial accuracy,
and symlink outputs are rejected. Execution or policy failures receive zero
partial credit. Reward is one only when every required comparison passes.
The verifier saves `grade.json`, `reward.json` and per-file cell differences.

## Versioned leaderboards

`leaderboard.py` derives one board per frozen population, language and
prompt tier. Task names, dataset packages and runtime references come from
`build.py`; Hub schemas and ranking come from `leaderboards/config.yaml`.
Historical populations in `task-populations.json` stay frozen. New tasks can
be evaluated immediately, then admitted to a new population version without
changing previous scores.

A row represents one job with one agent, model, variant and source commit.
Export requires every board task, equal attempts, finished trials, uniform
agent settings, the expected grading protocol and image references, and
unchanged task resources, timeouts, network and verifier. Export pins the
full source commit in the board identity and metadata. Jobs from different
commits are compared separately.

Boards rank by mean per-task reward, then cell accuracy and cost per
attempt. An errored trial counts as zero. Additional metrics include mean
row accuracy, token and cost totals, runtime, a bootstrap interval over
tasks, and Harbor's unbiased pass-at-k estimator when enough attempts exist.
Choose attempts before running compared models and keep task populations
and settings equal. Compare prompt tiers using paired runs; a brief score
includes guesses about sponsor choices omitted from that prompt.

For manual export of restored jobs:

```bash
uv run --project evaluations/harbor --no-sync python -m evaluations.harbor.leaderboard \
    export <board-name> <job-directory> --out /tmp/yamaa-harbor-export
```

The command prints the Hub create commands. Review the generated definition
and rows before running them. Preserve a published board's population,
source commit and runtime contract when adding compatible rows.
