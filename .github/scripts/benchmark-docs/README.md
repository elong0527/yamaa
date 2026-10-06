# Benchmark dashboards

Generates a self-contained HTML page per benchmark plus the
`docs/benchmark/index.md` gallery, from each benchmark's `README.md`, spec,
optional `define.yaml`, and `input/`/`expected/` files, plus its agent
evaluation prompts and reference solutions in
`evaluations/harbor/{prompts,solutions}/<name>/`. Output is git-ignored --
never edit it by hand.
Edit fixtures, the templates here, or the gallery prose at
`docs/articles/benchmark.md` (it holds the substitution placeholders, so
`exclude_docs` keeps MkDocs from publishing it), then regenerate and test:

```sh
uv run --with-requirements .github/scripts/benchmark-docs/requirements.txt python .github/scripts/benchmark-docs/generate.py --all
uv run --with-requirements .github/scripts/benchmark-docs/requirements.txt python -m unittest discover -s .github/scripts/benchmark-docs -p 'test_*.py'
```

Pass benchmark names instead of `--all` to regenerate only those pages, and
`--check` to compare bytes without writing. Pinned dependencies and sorted,
ASCII, timestamp-free output make generation byte-reproducible; the `Benchmark
dashboards` workflow proves it across two time zones and hash seeds. The
generator only displays expected artifacts, it never executes yamaa; comments
are giscus threads keyed by directory name, with the category ID in `GISCUS`.

A page asks clinical trial statisticians and programmers to challenge the
benchmark itself: is the task in the summary one a real study would need, and
does it follow CDISC; do the inputs look like collected trial data; would
independent QC programming reproduce the expected output? The summary, the
inputs, and the expected output carry an "Under review" chip, and every source
file has an Edit button for proposing a fix. Everything
else sits in one tab row under the datasets: Comments first, then the
assessment materials that are not under review -- the mapping spec generated
from the YAML, the agent prompt tiers, the yamaa code, and the solution written
without yamaa. A material a benchmark lacks has no tab. "Side by side" opens up
to three tabs in columns.
