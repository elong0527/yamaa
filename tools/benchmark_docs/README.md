# Benchmark dashboards

Generates a self-contained HTML page per benchmark plus the
`docs/benchmark/index.md` gallery, from each benchmark's `benchmark.yaml`,
`README.md`, spec, optional `project.yaml` codelists, and
`input/`/`expected/` files,
plus its agent
evaluation prompts and reference solutions in
`evaluations/harbor/{prompts,solutions}/<name>/`. Output is git-ignored --
never edit it by hand.
Edit fixtures, the templates here, or the gallery prose at
`docs/articles/benchmark.md` (it holds the substitution placeholders, so
`exclude_docs` keeps MkDocs from publishing it). Use the documentation checks in
[CONTRIBUTING.md](../../CONTRIBUTING.md#documentation); `make help` owns the
supported targets and interpreter prerequisites.

The gallery is the published benchmark index, and no index table is
committed: `benchmarks/README.md` keeps only its heading and prose. Gallery and
dashboard membership come from the public-conformance plan and each case's
authored exclusion metadata. `build_index.py` renders the same membership as a
Markdown table, with titles from case READMEs and lifecycle from
`benchmark.yaml`, to standard output or `--output <path>`; it never rewrites
the committed README, and `check_documentation.py` validates the rendered table.
Runnable Python/R entries are metadata; passing is gated by the native run,
where every active benchmark must pass on every lane.

The overview table, jump links and gallery list groups in the order `sdtm`,
`adam`, `schema`, `negative`, with one subsection per `benchmark.yaml` domain.
SEND examples belong to the `sdtm` tabulation group and retain their SEND
standard on their dashboards. README titles and specification contents do not
determine gallery categories.

Pass benchmark names instead of `--all` to regenerate only those pages, and
`--check` to compare bytes without writing. Pinned dependencies and sorted,
ASCII, timestamp-free output make generation byte-reproducible. The index test
checks that repeated rendering is stable. The
generator only displays expected artifacts, it never executes yamaa; comments
are giscus threads keyed by directory name, with the category ID in `GISCUS`.

A page asks clinical trial statisticians and programmers to review the
benchmark itself in a short survey under "Your review", as the Pilot 7
dashboards do: does the task follow CDISC as a real study would need it, do the
inputs look like collected trial data, and is the benchmark ready to finalize,
plus a free-text box for suggestions. The questions and their fixed choices are
declared once in `generate.py`; `review-form.js` reads them back from the page,
keeps a draft in the browser, and opens GitHub's new-discussion composer in the
Comments category with the answers filled in, so the static page holds no
credentials. The summary, the
inputs, and the expected output carry an "Under review" chip, and every source
file has an Edit button for proposing a fix. Everything
else sits in one tab row under the datasets: Comments first, then the
assessment materials that are not under review -- the mapping spec generated
from the YAML, the agent prompt tiers, the yamaa code, and the solution written
without yamaa. A material a benchmark lacks has no tab. "Side by side" opens up
to three tabs in columns.
