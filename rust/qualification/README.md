# Native benchmark qualification gates

The benchmark gate files below cover the complete Python-assisted native route for
[#1738](https://github.com/elong0527/yamaa/issues/1738). They do not change the
reference execution manifest, expected artifacts, compiler ownership or general
native readiness.

- `required.json` names 76 fixture/host/backend/level tuples that must pass both
  independent expected truth and the existing portable reference comparator.
- `known-gaps.json` names five unqualified validation/read-timing differences.
  Their exact findings must remain visible. A gap never counts as a pass, cannot
  satisfy a required tuple and cannot exempt reference or infrastructure errors.
  Any changed or disappearing difference requires explicit disposition.
- `missing-routes.json` identifies R's missing original-YAML entry point under
  #1739. No R execution is fabricated to fill that gap.

`supplemental.json` separately reconciles component/compile contract files and
installed Python/R probe suites. Its runner retains actual suite logs and results;
these do not enter the complete-run benchmark count. See the
[supplemental evidence contract](../SUPPLEMENTAL.md).

The initial set was selected by executing every fixture through the original
YAML, unchanged inputs and independent expected artifacts, then comparing all
portable observations with reference Python. It includes 16 positive and 60
negative cases; frontend validation failures still belong to the assisted route.
The installed wheel/source CI runs regenerate the evidence without generating
or editing these gates or any expected file. Reports identify the actual tested
revision, package versions, artifact names and workflow run. CI artifacts, not
this static list alone, prove a pass on a particular platform/revision.

To qualify another case, implement its missing behavior, obtain a complete
installed report matching independent truth and reference observations, and add
its exact route here. A shared-compiler route requires its own `shared_run` tuple;
an assisted pass cannot satisfy it. Retire a known gap only when its new evidence
has been reviewed. Keep the inventory's denominator synchronized with the
execution manifest rather than fixing it to today's 315 cases.
