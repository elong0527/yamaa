# Shared native engine bootstrap

This is the installation slice of [#1585](https://github.com/elong0527/yamaa/issues/1585).
It does not evaluate specifications. Both installed bindings call the same
`yamaa-engine`/`yamaa-core` code and report `execution_supported = false`.
The existing Python engine remains the default; the legacy R package is unchanged.

Decimal rounding characterization and the reference/rule discrepancies that
precede its implementation are recorded in
[ROUNDING_ASSESSMENT.md](ROUNDING_ASSESSMENT.md).

## Boundaries

| Crate | Responsibility | Allowed dependencies |
| --- | --- | --- |
| `yamaa-core` | Language contracts | Pinned `ryu` for formatting, `libm` for math and `num-bigint` for exact decimal rounding (all no_std) |
| `yamaa-engine` | Application entry points | Core |
| `yamaa-adapters` | Infrastructure, including embedded resources | Core, engine |
| `yamaa-python` | Python binding | Engine, adapters, PyO3 |
| `yamaa-r` | R binding | Engine, adapters, extendr |

Core and engine use `no_std` and forbid unsafe code. The dependency guard checks
all Cargo dependency kinds, including build, development, and target-specific
edges. New dependencies require a deliberate update to its allowlist. There are
no workflow application services yet. The numeric resolver port belongs to
the typed scalar evaluator; workflow ports will arrive with their first use cases.

The first core slice implements closed runtime values, validated civil temporal
values, present-value comparison, and basic arithmetic primitives. See
[`CAPABILITIES.md`](CAPABILITIES.md) for requirement-level coverage and remaining
gates. These primitives do not enable either host's dataset execution backend.

Completed-result scalar conversion now implements the closed column-type matrix.
Ryu 1.0.20 supplies shortest round-trip digits; core expands exponents to positional
text without floating-point math. It is a numeric library with no runtime
dependencies, not a host or table adapter. Rust's default float Display chooses
different digits from Python on some exact decimal ties; the shared fixtures
and `tools/check_float_text.py` protect the reference spelling. See the
[formatter documentation](https://docs.rs/ryu/1.0.20/ryu/) for its algorithm/API.

SQRT uses [libm 0.2.16](https://docs.rs/libm/0.2.16/libm/fn.sqrt.html), pinned
with default features disabled. This MIT-licensed pure-Rust no_std numeric library
has no runtime dependencies; its Rust 1.63 minimum is below this workspace's 1.90.
Disabling the default `arch` feature selects its generic square-root algorithm.
The dependency allowlist deliberately includes libm for this primitive; it does
not grant coverage to other math functions. No host math ABI is added. Cargo
transitive locking remains a separate release gate under the repository policy.

`evaluation::NumericPlan` evaluates the implemented numeric subset from a typed
tree through a core-owned resolver. It preserves operand order, association,
missing versus absent values, opaque resolution failures, and diagnostic
provenance. Shared evaluation fixtures compare values and resolution traces with
Python's real numeric parser/evaluator. The test-only postfix tree notation is
not an implementation of the numeric grammar.

`numeric_parser::parse_numeric` separately parses the full closed grammar into an
immutable arena. CI reads all 44 cases and the function/reserved-word tables from
`yaml/grammar/numeric.yaml`, then compares deterministic diagnostic/shape samples
against Python. Nodes retain exact UTF-8 source spans, literal spelling, function
names and grouping; errors carry both byte and Unicode scalar offsets. Numeric
range errors are deferred to future compilation/evaluation, not raised by parsing.

Parsing defaults to 65,536 source bytes, 8,192 tokens (excluding EOF), 4,096 nodes
(including groups), and depth 64 (a leaf has depth one). Callers can choose budgets;
depth is always capped at 64, including descent before a node exists. These are
prototype resource policies, not additions to the language grammar. Limit outcomes
remain separate from grammar conditions. Lexing completes before syntax checking
unless a budget is exhausted. The arena avoids recursive destruction of rejected
input. Parsing never resolves identifiers or invokes host code.

`numeric_compiler::compile_numeric` now connects text to the supported evaluator
subset: literals, identifiers, unary signs, arithmetic, ABS, MOD, GREATEST, LEAST,
NULLIF, COALESCE, CEIL, FLOOR, TRUNC and SQRT. It checks all
functions before producing a plan; other valid functions return explicit
`Unsupported` entries with their written name spans, in source order. Compilation
never invokes a resolver. It preserves association and defers oversized integer
literal failures until evaluation reaches them, including before unary negation.

Compiled plans are immutable and inherit the parser's byte/token/node/depth
budgets. A separate static budget counts identifier occurrences (default 4,096),
including repeated names, before any evaluation. It bounds calls per evaluation,
not callback duration or cumulative work across plan reuse. Limits are policy
outcomes rather than language conditions; unsupported functions take priority
over the resolution budget after successful parsing. The public caller-supplied
`NumericPlan` remains an internal, unbounded construction API.

Evaluation failures preserve the original text, specification path, operand route,
opaque resolver payload and failed node's UTF-8 span. Groups do not overwrite an
inner node's location. Existing arithmetic/trace fixtures now run through both
typed trees and compilation; 22 shared literal cases compare exact results with
Python. Another 51 shared selection cases cover eager argument traces,
missingness, promotion, signed zeros, i64/binary64 boundaries and nested failures.
Default-policy EXP/LN/POWER restrictions, lifecycle handlers and host diagnostic
transport are the next gates.

CEIL, FLOOR and TRUNC propagate missing and always return float, even for integer
inputs. Integral zero results are positive zero, matching the reference's integer
intermediate. The primitive clears fractional binary64 bits and adjusts toward the
requested direction; it does not narrow large floats through i64 or add a math
library dependency. All finite values of magnitude at least 2^52 are already
integral. There are 100 shared cases for types, boundary values and failure order,
plus exact standard-library comparisons across every finite exponent, both signs,
significand boundaries and 100,000 deterministic bit patterns. Remaining
transcendental functions still require their own dependency and numerical
policy decisions; these exact tests do not establish their parity.

SQRT promotes present input to binary64 and always returns float. It preserves
negative zero, propagates normalized missing values and reports `sqrt_of_negative`
under REQ-0431 for negative present values. The domain failure belongs to the call's
source span; failures inside its operand keep their own span and occur first.
There are 32 independently specified shared result/failure cases. CI additionally
runs `tools/check_sqrt.py` against Python math.sqrt on Linux/macOS/Windows: exact
bits for 110,188 inputs spanning signed zero, all finite exponents, significand
boundaries, i64 promotions and deterministic samples. This check writes no fixtures
and allows no tolerance. It qualifies this sample and these targets, not untested
math functions, arbitrary floating-point modes or full backend execution.

Numeric selection evaluates every argument in written order before selecting a
result, including COALESCE after its first present argument. A later failure still
fails the expression. GREATEST/LEAST compare present values exactly, retain the
first equal value, then promote the result. NULLIF compares integer pairs exactly
and mixed pairs after binary64 promotion, matching the reference. The compiled
resolver budget counts every argument occurrence, not just the selected one.

This slice deliberately corrects Python COALESCE promotion under REQ-0424:
`COALESCE(1, 2.0)` returns float `1.0`, rather than int `1`. Any present float
argument promotes a selected integer, including its normal rounding above 2^53.
Missing arguments do not influence promotion. This can change dependent arithmetic
and completed-result conversion for large integers; the shared vectors pin it.

## Packaging decision

The optional `yamaa-native` wheel uses Maturin; `python/` retains its existing
Hatch build. Import the probe with `import yamaa_native`. Installing it does not
change `yamaa` backend selection or conformance coverage.

The optional R package is `yamaanative`, alongside `cdiscbuilder`. It has its own
`DESCRIPTION`, registered native routine, help page, and source installation.
Its source archive embeds the workspace via `tools/stage_r_package.py`, so there
is one maintained copy of the Rust code. Build the staged package, not the
incomplete source template in `R/yamaanative` directly. The installed package
does not need the repository, Cargo, Python, or R's legacy package dependencies.

The bootstrap resource is embedded in the binary and tested after installation
outside the checkout. This tests packaging only, not discovery of study resources.

Rust 1.90.0, PyO3 0.27.2, extendr 0.9.0, and Maturin 1.9.6 are pinned. Cargo's
generated lockfile is ignored: the repository's current no-hashing rule permits
only `python/uv.lock` as a packaging exception. Consequently, transitive Cargo
dependencies are **not reproducibly locked** in this slice. Source distributions
may contain Cargo-generated packaging checksums; these are never runtime,
contract, or resource identities. A reviewed lockfile policy remains a step 3
release gate. Do not claim reproducible release builds from this prototype.

## Development and installation checks

Install Rust with rustup and the host's C compiler. Enter `rust/` before invoking
Cargo so `rust-toolchain.toml` selects the pinned toolchain. Build R bindings with
R installed and on `PATH`.

```sh
cd rust
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
python tools/check_dependencies.py
python tools/check_float_text.py
uv run --project ../python --locked python tools/check_numeric_grammar.py
python -m unittest discover -s tests -p 'test_*.py'
```

From the repository root, with a new empty output directory:

```sh
uv tool run --from maturin==1.9.6 maturin build \
  --manifest-path rust/crates/yamaa-python/Cargo.toml --release --sdist \
  --out /tmp/yamaa-dist
python rust/tools/stage_r_package.py /tmp/yamaa-stage/yamaanative
cd /tmp/yamaa-dist
R CMD build --no-build-vignettes --no-manual /tmp/yamaa-stage/yamaanative
R CMD INSTALL --library=/path/to/empty/R-library yamaanative_*.tar.gz
```

Use `cargo +1.90.0` or `RUSTUP_TOOLCHAIN=1.90.0` when launching a build from
outside `rust/`. Test both a wheel install and a separate install from the Python
source archive. `tests/installed_python.py` and the R package's
`tests/installation.R` assert the installed capabilities and resource contents.

CI builds Python 3.12/3.14 on Linux x86_64, macOS arm64, and Windows x86_64;
the stable ABI wheel targets Python 3.12+. It builds R 4.6.1 on Linux x86_64 and
macOS arm64. R Windows native installation and other architectures are not
qualified by this slice. The existing Windows Python engine remains supported.

The initial hosted installation matrix passed. Next gates: settle transitive
dependency locking, complete typed evaluation/grammar/diagnostics, then add
Arrow tables and host callbacks. No performance or language-parity claim is made.

Local `R CMD check --no-manual --no-build-vignettes` passes installation and
tests, but reports one compiled-code warning for Rust's linked `_abort` symbol.
The prototype is not CRAN-qualified. R process safety and panic/condition
translation still require the explicit boundary tests in step 5 of #1585.

## Remaining math compatibility gate

`EXP`, `LN` and `POWER` remain unsupported by default `compile_numeric`. The candidate
libm functions do not reproduce Python platform math bit-for-bit on the assessed
sample. `tools/assess_math.py --output /tmp/math-assessment.json` measures this
without modifying expected fixtures or accepting a numerical tolerance. Run it
from `rust/` with Cargo and Python available. It samples 10,011 inputs per function,
including range boundaries, observed mismatch inputs and a deterministic sequence.
Only finite EXP inputs, positive LN inputs and positive POWER bases are included;
domain diagnostics and the remaining negative/zero POWER cases require separate
qualification.

On macOS arm64 with Python 3.14.7 and libm 0.2.16 (default features disabled), the
sample has 989 EXP, 279 LN and 950 POWER mismatches. Each is one adjacent binary64
value apart. That is a measured difference, not an accepted error bound or proof
that either implementation is correctly rounded. The exact operands and both
results are retained in the JSON report, along with host/version information.
The native Python CI matrix uploads separate reports for Linux/macOS/Windows and
both Python versions; results may differ with the platform math implementation.

A successful assessment process means the report was produced. Its qualification
is `blocked-by-mismatches` when any difference exists, otherwise `not-qualified`:
a finite sample cannot establish complete parity. Domain/missingness, zero sign,
finite bit differences and ULP distance must not be collapsed into an overall pass.
The report is observational evidence and never regenerated expected truth.
Schema 2 also retains every candidate observation for exact cross-platform checks.

Before enabling these functions, select and document a common numerical policy:
either reproduce the supported reference behavior, or make an explicit shared
semantics change and qualify both hosts, dependent calculations, rounding and CSV
outputs against independent truth. Do not enable libm behind a broad tolerance.
Decimal expression rounding now follows the exact rule described in
[ROUNDING_ASSESSMENT.md](ROUNDING_ASSESSMENT.md); broader qualification remains a
separate implementation gate. Cargo locking and dataset execution remain later
release gates.


An explicit opt-in is now available through `compile_numeric_with_policy` and
`MathPolicy::PortableLibmV1`. The plan retains that choice before resolution;
default compilation remains ReferenceSubset. See [the numerical policy](MATH_POLICY.md)
for its deliberate distinction from historical platform Python, domain contracts,
exact cross-platform CI checks and remaining qualification gates. The assessment
probe now evaluates the compiled policy, with no golden regeneration or relaxed
SQRT checks. Backend availability and Python production behavior do not change.


## Exact decimal rounding

ROUND_HALF_AWAY_FROM_ZERO is implemented in primitives, typed evaluation and
bounded compilation under both math policies. It preserves the corrected
REQ-0418 inclusive near-tie interval, float result type, positive zero, missing
propagation, eager operand errors and integer-digits validation. The 34 shared
vectors run in Rust/Python; CI checks 5,872 compiled results (2,936 inputs under
each policy) against the independent rational oracle on every native platform.

Exact intermediates use [num-bigint 0.5.1](https://docs.rs/num-bigint/0.5.1/num_bigint/),
pinned with default features disabled (MIT OR Apache-2.0, MSRV 1.60). Its numeric
runtime dependencies are num-integer and num-traits; autocfg is build-only.
No std, random, serialization, host or table features are enabled. Digit counts
outside [-309, 340] return before decimal powers are constructed. Within that
range, intermediates are at most 2,155 bits and the selected decimal integer at
most 649 digits. Runtime integer values remain i64; big integers are private
rounding intermediates. Final decimal-to-binary64 conversion uses Rust's float
parser once, followed by nonfinite and positive-zero normalization.

The dependency allowlist explicitly admits this numerical helper. Transitive
Cargo locking remains the existing release gate; this PR adds no lockfile or
repository content digest. Host APIs, standalone specification dispatch, result
conversion/handlers and complete dataset execution remain separate integration.
