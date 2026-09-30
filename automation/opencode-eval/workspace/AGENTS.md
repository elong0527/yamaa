# Workspace

This workspace holds one clinical-data programming request. The request
itself is the message you were given; it names the files here and what to
deliver.

## Tools

- The yamaa Python engine is installed in the active `python`.
- `python yamaa_run.py` runs the specification in this directory: `spec.yaml`,
  or the one `spec_*.yaml` no other specification names as a parent or
  producer. It clears `output/` (keeping `NOTES.md`), publishes every artifact
  a successful run produces into `output/`, and prints the outcome as YAML:
  the published files, or the engine's diagnostics.
- `python -m yamaa.style <specification>.yaml` checks the specification
  style contract.
- `reference/` holds the language: `reference/yaml/` is the schema bundle,
  `reference/rules/` the rules that fix what each field means, and
  `reference/articles/` the introductions. `reference/python.md` describes the
  Python API.

## Ground rules

- A deliverable in `output/` must be what `python yamaa_run.py` published.
  Do not write, copy, or edit a dataset there any other way.
- Set each specification's `output.path` to the bare file name the request
  asks for (for example `adae.csv`); `yamaa_run.py` places it in `output/`.
- Do not edit, rename, or delete anything under `input/`, or any file the
  request lists as given.
- When the run cannot produce the dataset because the specification or the
  data breaks a stated requirement, do not work around it. After your last
  run, copy the first diagnostic `yamaa_run.py` printed -- all of `phase`,
  `condition`, `spec_paths`, `requirement`, and `context` -- into
  `output/error.yaml`, and explain the cause and the change you would
  propose in `output/NOTES.md`.
- Everything you need is in this directory. Do not fetch anything from the
  network.
