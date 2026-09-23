# Project-Root Inherited Input

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-inherited-input-path.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** show how a shared layer declares an input dataset once
while the project supplies the file. `shared/base.yaml` names
`input/dm.csv`; the entry sits under `yamaa-project.yaml`, so
the written path keeps its spelling and the run reads
`input/dm.csv` from the project root.

**Dataset:** `out.csv`, pass-through of the inherited input. One
record per collected row, carrying `ID` and `VAL` unchanged.

**Input:** two spec files:

- `shared/base.yaml` is the shared layer: it declares the `DM`
  dataset with `path: input/dm.csv` once;
- `spec.yaml` is the entry: it names the shared layer as its
  parent and sits under `yamaa-project.yaml` at the benchmark
  root, so the written path resolves from the project root.

The layer's own folder holds a decoy `shared/input/dm.csv`
with different rows. Had the path fallen back to the layer's
folder, the run would read the decoy; the golden carries the
project root's rows, proving the project file won.

**Standard:** -- | **Domain:** PATH
