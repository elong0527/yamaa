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

SAS transport (`.xpt`) files have no preview pane; their CSV companions show
the data for review, and the transport files remain in the benchmark source.
Define-XML 2.0 and 2.1 documents display resolved variable and value-level
metadata, including codelists, origins and methods. Other XML stays literal,
including intentionally malformed inputs. HTML files (`.html` and `.htm`)
render in an isolated frame with inline styles and data images; scripts and
external resources are disabled. Document previews span the dataset area and
keep the original source in a collapsed disclosure below the rendered view.

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
