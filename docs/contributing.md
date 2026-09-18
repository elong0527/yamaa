---
title: Contributing
---

# Contributing to the YAMAA documentation

These pages are built with [MkDocs](https://www.mkdocs.org/) and the
[Material for MkDocs](https://squidfunk.github.io/mkdocs-material/) theme. The
site configuration lives in `mkdocs.yml` at the repository root; page sources
live under `docs/`.

## Preview locally

From the repository root:

```sh
python -m venv .venv-docs
source .venv-docs/bin/activate
pip install -r docs/requirements.txt -r .github/scripts/example-docs/requirements.txt
python .github/scripts/example-docs/generate.py --all --quiet
mkdocs serve
```

`mkdocs serve` starts a live-reloading preview, by default at
`http://127.0.0.1:8000`. The `generate.py` step builds the benchmark pages
into `docs/benchmark/` (git-ignored generated output): one dashboard per
example plus the [benchmark](benchmark/index.md) page itself, which is in
the nav, so `mkdocs build` fails without it.

## Build strictly

```sh
mkdocs build --strict
```

The deployment workflow builds with `--strict`, so every link and every nav
entry must resolve with zero warnings before merging.

## Conventions

- Guides go in `docs/articles/`; reference indexes go in `docs/reference/`.
- Every page under `docs/` must appear in the `nav` section of `mkdocs.yml`.
- Do not edit generated output under `docs/benchmark/` by hand. Edit the
  example fixtures under `benchmark/` or the generator under
  `.github/scripts/example-docs/`, then regenerate.
- Do not edit normative content (`yaml/rules/`, the schema bundle, runnable
  examples) as part of a docs change.
- Comments on example dashboards are powered by
  [giscus](https://giscus.app) and stored in this repository's GitHub
  Discussions. The comment theme lives at `docs/assets/giscus-yamaa.css`.
