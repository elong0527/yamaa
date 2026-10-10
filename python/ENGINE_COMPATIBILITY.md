# Engine migration compatibility

This is the compatibility inventory for [#1585](https://github.com/elong0527/yamaa/issues/1585).
The Python implementation remains the default and reference during preparation.
The rules and independently reviewed fixtures define semantics; an implementation
disagreement requires investigation rather than automatic golden regeneration.

## Preserved interfaces

| Surface | Required behavior |
| --- | --- |
| `yamaa.yamaa_domain` | Explicit schema, project and data roots; optional project configuration and study document; one cached workflow execution. |
| `DomainRun` | `spec`, declaration-ordered `inputs` and `input` alias, Polars `output`, `issues`, warning and verification logs; returned frames are independent copies. Failed runs have no accepted output. |
| `DomainRun.save` / `DomainRunError` | Explicit publication only, output gates, requested transport extension, atomic replacement, and structured issues on unsuccessful runs. |
| `yamaa.domain` / `yamaa.check` with `environment=` | Shared original-file preparation, static diagnostics without code/data, fresh lock verification and called-function tests before each build, retained issues/output and explicit publication. The superseded `functions` runner, artifact resolver and activation cache are retired under #1757. |
| `ExpressionDispatcher` | Caller-supplied supported-operation registry and nested dispatch. An unsupported extension must be reported before execution, never silently ignored or retried with another backend. |
| `ExecutionHooks` | Column, key, dataset and output hooks; existing three-argument verification hooks and records-aware hooks. |
| `specification.load_specification` | Strict current-schema validation, normalized models and portable diagnostic paths. Python model compatibility must be decided before replacing exposed objects with Rust wrappers. |
| ODM and submission helpers | Preserve `iter_odm_records`, `read_odm`, `write_odm_parquet`, and `generate_study_document`. XML/JSON rendering can remain in adapters. |

Internal planner and evaluator classes are migration seams, not a commitment to
reproduce the same internal object graph in Rust. Existing tests remain valuable
even when their implementation-specific setup changes.

## Explicit behavior improvements

Preparation fixes workflow-wide function activation: a function used only by a
producer must work through the public runner just as it does through benchmark
execution. Preparation also exposes a retained failure verification log through
`DomainRun.verification_log`; doing so does not make failed output publishable.
Each improvement needs a focused regression, separately from behavior-preserving
extraction. Numeric rounding policy changes are outside this cleanup.

## R compatibility

`R/cdiscbuilder` currently exposes legacy `create_sdtm_datasets`, `process_domain`,
`build_adam_dataset`, `topological_sort`, ODM parsing, submission validation, and
clinical helper functions listed in `NAMESPACE`. Retain these exports during
preparation. Do not reinterpret legacy `key`/`data_dependency` specifications as
the current schema. Before retiring an export, provide a tested exact translation
or a documented deprecation and replacement.

R grammar-vector tests prove parser agreement only. The execution manifest at
the initial assessment declares all 315 examples for Python only. Neither issue
closure nor an R package test pass establishes current-schema R execution parity.
The future R facade must preserve full-range i64 and missingness; ordinary R
numeric vectors are not an adequate default interchange contract.

## Migration checks

Preserve types, values, row/column order, missingness, diagnostics, handler counts,
evaluated verification records and callback order/counts. Compare exact CSV bytes
and logical Parquet data. Check source and prepublication tables as well as final
artifacts so reporting precision cannot hide a change to dependent calculations.

Reports must identify host language and engine backend separately. Select a
backend before a run has side effects; never fall back after invoking callbacks.
Unsupported coverage is explicit, and reference reports are observations rather
than replacements for independently committed expected artifacts.

Public environment functions use the ordinary packages installed in the uv/renv
locked host. yamaa does not resolve artifacts or cache activation; every build
repeats called-function tests. Resource snapshots compare held bytes directly.
Keep AGENTS.md unchanged: only the existing `python/uv.lock` carries tool hashes;
do not introduce content hashes or new digest-bearing locks during migration.
