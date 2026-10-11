# POWER, EXP and LN decision for #1740

The maintainer ratified pinned `libm` 0.2.16 with default features disabled on
2026-10-10, replying "agree with your recommendation" in the Rust migration
assessment. The [existing decision record](1757-maintainer-decisions.md) retains
that authorization. This decision milestone selects the implementation and
disposes of documented historical differences; full release qualification and
transition remain #1742/#1585. This evidence change alters no production policy,
API, rule or benchmark expected artifact.

One locked yamaa release supplies one numerical behavior in both hosts and every
compute context. Public APIs and YAML select neither policy nor backend. Existing
specification text alone does not preserve historical platform bits: replay an old
run with its prior locked package, inputs and relevant platform environment.
Rollback pins that prior release before a fresh run; a running build, callback
continuation or Save cannot change policies or fall back to another evaluator.

The baseline is Rust 1.90.0, ordinary binary64 operations, written association,
round-to-nearest/ties-to-even and enabled subnormals. Changing library, features,
algorithm, compiler or qualified target requires requalification. Observable
changes to bits, missingness, zero sign, failure order, conversion or handler
behavior require an explicit compatibility decision, a new release and renewed
qualification. Other floating-point environments remain unqualified. The
internal `PortableLibmV1` label identifies evidence, rather than an additional
public version parameter or mandatory result field.

| Considered policy | Disposition |
| --- | --- |
| Pinned shared libm | Approved: bounded implementation and qualification path; accepts the documented historical compatibility break. No universal correct-rounding or global error-bound claim. |
| Require a correctly rounded replacement first | Deferred alternative: select and independently qualify another implementation before enabling it. Historical bits could still change; build and performance costs need separate evidence. |
| Preserve each platform's historical math | Prior locked packages remain the replay route. Different platform implementations cannot by themselves provide the required shared behavior. |

## Retained evidence and method

[math-decision-v1.json](../qualification/math-decision-v1.json) retains exact input
bits, every known sampled historical mismatch, independent accuracy certificates,
complete boundary/lifecycle observations and actual downstream artifact witnesses.
Its sample source is [PR #1917's final hosted run](https://github.com/elong0527/yamaa/actions/runs/38107594072),
with nine Linux x86_64/macOS arm64/Windows AMD64 reports for Python
3.12/3.13/3.14. The independent `numeric-math-v1` case/input gate and exact comparison
pass all 30,033 observations per report. Candidate host equality, historical Python
compatibility, mathematical accuracy and artifact equality are separate claims.

The compressed union retains all 2,246 mismatching inputs and all nine historical
results, including hosts which matched the candidate at an input. The observed
historical differences are finite adjacent-bit differences: EXP 1,006 inputs,
LN 280 and POWER 960. The report records each exact reference/candidate pair;
no tolerance turns them into compatibility passes.

Independent certificates enclose correctly rounded 160-digit Decimal EXP/LN or
pure Python `_pydecimal` POWER results within half a decimal ULP, then compare
that entire enclosure to exact rational binary64 rounding-cell bounds. Bounded
integral powers use exact rational arithmetic, including half-minimum-subnormal
ties. Expected bits are selected from those bounds before inspecting candidate
bits. These certificates rely on the documented
[Decimal EXP/LN](https://docs.python.org/3.14/library/decimal.html#decimal.Decimal.exp)
and [Python POWER contracts](https://docs.python.org/3.14/library/decimal.html#decimal.Context.power);
they are independent of libm, rather than a formal verification of Decimal.
The C Decimal POWER implementation's weaker guarantee is deliberately unused.
All 2,246 selected inputs receive numerical certificates. Candidate nearest counts
are EXP 23/1,006, LN 0/280 and POWER 7/960; this biased union says nothing about
overall error rates. The approved choice prioritizes shared behavior and does
not claim superiority to platform math.

The separate [math-decision-v1 input specification](../tools/math_decision_corpus.py)
contains 58 deterministic cases. Thirty-three have numerical certificates
(31 candidate matches and two discrepancies); 21 have separate language truth
for identities, zero signs, missingness or normative domain refusals. Four
explicit accuracy exclusions are EXP of the largest positive/negative finite
inputs and POWER of the neighbors of one to exponent 2^52. They exceed the
certificate work bounds: EXP input magnitude 1,000, POWER exponent magnitude
2,048, result log2 enclosure 4,096 or exact rational size 16,384 bits. No excluded
case is counted as mathematically correct. The existing 38-case semantic corpus
separately retains eager arguments, resolver failures, types and failure order.

The earlier issue proposal described a local 334-tuple/1,002-observation experiment,
eleven differences and 244 certificates. Its complete inputs and scripts could not
be recovered from the available evidence locations. Those counts are not used
as acceptance evidence. The retained 58-case corpus is an explicitly specified
replacement; it makes no claim to reproduce the unavailable experiment.

## Compatibility disposition

| Known observable class | Exact evidence | Approved disposition |
| --- | --- | --- |
| Finite result bits for EXP/LN/POWER | Every 2,246 union input and nine historical results, with independent certificates | Accept the new release behavior. Preserve each mismatch; one observed ULP is not a tolerance or a global accuracy bound. |
| Dependent predicate and verification | EXP(1) changes `VALUE > 2.718281828459045` from false to true; a separately executed `VALUE <= ...` assertion changes from success to full REQ-0384 failure | Accept the documented downstream compatibility cost. Keep exact failure facts and keys. |
| Decimal rounding and canonical CSV | EXP(1) changes the 15-digit rounded value from `2.718281828459045` to `2.718281828459046`; both complete CSV byte strings are retained | Accept the resulting rendered-byte difference for new inputs/runs. Original positive benchmark goldens remain unchanged. |
| Integer conversion and replacement handler | POWER(196.96327154667048, 6.787359008266933) changes historical replacement 9/count 1 into native integer 3739524560436070/count 0 | Accept the documented conversion/handler change. Its independent certificate favors the historical nonintegral result; no accuracy claim hides that cost. |
| Opposite conversion/handler direction | POWER(base bits `3fcfffffffffffff`, -1) changes historical integer 4/count 0 into native replacement 9/count 1 | Accept the same release compatibility cost. This independent rational certificate favors the native nonintegral result. |
| Missingness, zero signs, promotion and domains | Exact bounded cases plus the existing shared semantic truth | Preserve existing normative language rules. No historical discrepancy was observed in these selected cases; broader claims remain unqualified. |

The downstream EXP witnesses feed the two exact observed results as source data
into the actual shared predicate, verification, rounding and serializer. This
isolates downstream effects and does not emulate platform math in a public run.
Both POWER witnesses execute the actual retained Python completed-value lifecycle
and the native numeric lifecycle with declared zero/fired handler counts.

The five affected current fixtures are `adam-adeg-derived-intervals`,
`schema-numeric-functions`, `negative-log-zero`, `adam-adsl-bmi` and
`adam-advs-percentiles`. The current retained installed Python replay preserves
three complete CSV goldens and the complete negative LN diagnostic. #1757 also
qualified the original callback/POWER `adam-adsl-bmi` and `adam-advs-percentiles`
through both installed hosts with unchanged truth. `adam-advs-bmi` is a different
fixture whose expression uses ordinary multiplication, so its prior success
never supplied evidence for the callback/POWER fixture.

`adam-adeg-derived-intervals` still returns `unsupported_operation` at
`rows[1].derivations.QTVAL.aggregate.filter`. The report preserves that refusal.
Its explicit retained-reference planning experiment executes eight original math
calls under each primitive and preserves the whole original CSV. That experiment
isolates the numerical choice; it does not qualify the shared filtered-aggregate
frontend or count the native refusal as a pass.

## Qualification follow-up

1. #1751 completes the original filtered aggregate frontend; replay
   `adam-adeg-derived-intervals` through both installed hosts against its unchanged
   golden. Keep its current refusal visible until that succeeds.
2. #1742/#1585 qualify the final declared host/platform support matrix and every
   final compute scope. Replay the independent semantic cases and the retained
   boundary/downstream witnesses with exact values, diagnostics, failure order,
   conversion, handler accounting and rendered bytes in installed Python and R.
3. #1742 qualifies the complete affected fixture inventory and any newly discovered
   downstream consumers. Preserve original expected truth; separately justify and
   review any intentional semantic correction before changing a golden.
4. #1742 records exact release/build/artifact evidence and floating-point
   environment assumptions, proves every public API uses the same behavior and
   demonstrates rollback by a prior locked release before a fresh run. Migrate
   qualification consumers before removing experimental scalar selectors.
5. #1742 owns representative performance/memory measurements and broader
   difficult-domain accuracy work. This decision supplies neither a speedup claim
   nor an all-input accuracy or portability certificate.

Reproduce the evidence with the retained nine reports and an installed package:

```sh
python rust/tools/assess_math_decision.py \
  --reports /path/to/nine-hosted-reports \
  --hosted-run https://github.com/elong0527/yamaa/actions/runs/38107594072 \
  --artifact-reference yamaa-0.3.0-cp312-abi3-macosx_11_0_arm64.whl \
  --source-commit 06f0dcdb829a7e35b3d5bb43c1cec2c015820df8 \
  --output /tmp/math-decision-v1.json
```

The checked-in local report identifies that exact installed wheel and commit on
macOS arm64/Python 3.14. It retains observations without content hashes. Its
certificate and decision-evidence tests validate independent input identities,
all mismatch classes, explicit exclusions, opposing handlers, full downstream
diagnostics/CSV and direct original-golden equality. Neither this script nor those
tests writes a benchmark expected artifact.
