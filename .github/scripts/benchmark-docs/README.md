# Benchmark dashboards

Generates a self-contained HTML page per benchmark plus the
`docs/benchmark/index.md` gallery, from each benchmark's `README.md`, spec and
`input/`/`expected/` files. Output is git-ignored -- never edit it by hand.
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
