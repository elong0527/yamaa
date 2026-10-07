# Shared native engine bootstrap

This is the shared engine prototype for [#1585](https://github.com/elong0527/yamaa/issues/1585).
It supports bounded opt-in execution, not full specification compatibility.
Both installed bindings call the same
`yamaa-engine`/`yamaa-core` code and report `execution_supported = false`.
The existing Python engine remains the default; the legacy R package is unchanged.

The executable fixture inventory and its evidence gates are described in
[QUALIFICATION.md](QUALIFICATION.md). Missing native reports remain visible;
component probes do not imply full benchmark qualification.

The core-owned numeric/conversion diagnostic model, semantic-cause registry and
remaining cross-family migration are described in [DIAGNOSTICS.md](DIAGNOSTICS.md).

Decimal rounding characterization and the reference/rule discrepancies that
precede its implementation are recorded in
[ROUNDING_ASSESSMENT.md](ROUNDING_ASSESSMENT.md).

The bounded typed predicate evaluator, its independent truth/trace tests and
parser/dataset integration are described in [PREDICATES.md](PREDICATES.md).

Opt-in dataset stage instrumentation and the installed, fresh-process measurement
procedure are described in [PHASE_MEASUREMENTS.md](PHASE_MEASUREMENTS.md).

Shared cycle selection, stable scheduling and column dependency rules, connected
to actual optional native planning, are described in [DEPENDENCY_ANALYSIS.md](DEPENDENCY_ANALYSIS.md).

Shared name binding, bare-output and direct qualified-field validation use an immutable catalog during
optional native planning; see [REFERENCE_BINDING.md](REFERENCE_BINDING.md).

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
edges. New dependencies require a deliberate update to its allowlist. There is
a numeric completed-result application service. The numeric resolver port belongs
to the typed scalar evaluator; table and publication ports remain future work.

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
NULLIF, COALESCE, CEIL, FLOOR, TRUNC, SQRT and ROUND_HALF_AWAY_FROM_ZERO.
It checks all functions before producing a plan; other valid functions return explicit
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
Default-policy EXP/LN/POWER restrictions, host diagnostic transport and full
specification dispatch remain gates.

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

## Numeric completed-result lifecycle

`yamaa-engine::numeric_lifecycle::NumericDerivation` binds an immutable compiled
numeric expression to a destination column type and an optional literal
`unconvertible` handler. It evaluates once, converts the completed result, and
only on conversion failure converts the selected replacement once to the same
type. An explicit null replacement produces Missing; an absent handler fails.
Missing results do not fire a handler, unused replacements are not converted,
and arithmetic, validation or opaque resolver failures bypass this handler.

A failed replacement retains both conversion errors and both declaration paths.
The run-local `HandlerCounter` records a firing before replacement conversion,
including failed replacements, and preserves first declaration order. Callers
register all plans before evaluation to retain zero counts for unvisited plans;
re-registration is idempotent. Count overflow is an explicit resource failure,
not a new language condition or silently saturated audit data. Reusing a plan
repeats resolver effects; callers own publication of completed values and there
is no callback rollback.

The shared 31-case corpus is hand-written truth replayed through this service and
Python's real lifecycle. It checks types, exact float bits, temporal precision,
conversion paths/contexts, resolution order and handler counts. Additional Rust
tests cover declaration order, repeated firings, opaque failures, count overflow
and a dependent plan consuming a converted replacement. CI runs core and engine
in debug and release profiles on all native Python targets.

This service accepts already normalized declarations. It does not decode a
specification, plan dependencies, execute tables, implement expression-local
handlers, verify columns or publish output. Installed hosts now expose this service through an explicitly versioned normalized
numeric request. The full workflow, table and callback gates remain open.

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

Both installed hosts additionally expose `scalar_round_trip` for the versioned
`scalar/1` envelope documented in [the R package README](../R/yamaanative/README.md#scalar-transport-probe).
The adapter decodes a strict JSON envelope into an actual core value and encodes
owned text, preserving full i64, exact finite binary64 bits, Unicode/escaped NUL,
missing versus empty text, booleans and temporal collected precision. Nonfinite
floats normalize to missing. This is a scalar boundary probe, not a new backend,
callback API, diagnostic format or table interface. Python raises ValueError for
invalid requests; R raises a condition only after the Rust call returns normally.

Transport uses pinned [serde 1.0.228](https://docs.rs/serde/1.0.228/serde/) and
[serde_json 1.0.145](https://docs.rs/serde_json/1.0.145/serde_json/) in adapters only.
Both are MIT/Apache-2.0 with MSRVs 1.56/1.61 below the workspace's 1.90. Derive is
explicitly enabled for serde; JSON uses standard features and its recursion guard.
The dependency allowlist admits these serialization libraries only in adapters.
Core and engine remain no_std without serialization dependencies. The transitive
locking release gate also covers these new adapter dependencies.

Requests are capped at 1 MiB before parsing; decoded scalar data and output are
bounded by that input (output escaping can expand text). Unknown/duplicate fields,
wrong scalar types, noncanonical numeric encodings and unknown versions fail
explicitly without echoing input. Unwind panics inside the adapter are contained
as internal failures; the unit test does not qualify host callback panic recovery,
process aborts or allocation failure. The 52 independent shared transport cases
run against Rust and installed Python wheel/source and R source packages. R uses
its own registered native entry point without Python or a JSON-package dependency.
Repeated calls test independent ownership and recovery after rejected requests.
Arrow ownership and installed callbacks are now qualified (see below); host panic
recovery, process aborts and allocation failure remain unqualified.

The installed `evaluate_numeric` API now composes core compilation and the engine
numeric lifecycle through a strict `numeric/1` adapter protocol. See the
[request/outcome contract](../R/yamaanative/README.md#numeric-application-prototype).
It reuses the scalar codec for bindings, completed values and diagnostic context;
diagnostic-only wide integers have a distinct decimal representation. Grammar,
unsupported-function, resource-limit and runtime failure outcomes stay separate.
Original expression/path/source geometry, written resolution traces, handler counts
and both replacement/original conversion failures are retained. Bindings are static
caller-owned data: no callbacks, file access, table execution or publication occurs.

The 59-case shared corpus extends the independent lifecycle truth with compile,
policy, diagnostic, input-validation and Unicode-position cases. Rust and installed
Python/R packages compare exact responses; 47 applicable cases also replay through
Python's real lifecycle, comparing values and complete primary diagnostic context.
Rust-only source spans and original replacement context are independently pinned.
Long exact errors, limits and repeated fresh accounting have additional tests.
The native execution flag still means specification/dataset support and remains
false. No default backend, existing production Python behavior or benchmark golden
changes are implied by this normalized numeric prototype.

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

## Ordered table access and numeric reductions

Core `TableAccess` describes an immutable snapshot with an ordered closed schema,
row count and borrowed normalized cells. Text stays borrowed from the snapshot;
i64, finite binary64, missingness and temporal precision retain their exact
representation. Invalid coordinates and opaque access errors are distinct from
missing. Schema names are unique and nonempty; identifier binding remains a
compiler responsibility. Empty tables retain every declared column and type.

The first consumer is engine `reduce_column`, for a bare bound column and an
already selected relation. It rejects duplicate or reordered row indices: a
selection is a strictly increasing subsequence of the snapshot, preserving
REQ-0471/0480. Upstream sorting would create a new relation. A caller-supplied row
budget, column bounds and row selection are checked before allocation or reads.
Allocation failure is a resource outcome. This budget bounds selection storage
and read count; it cannot bound time spent inside an arbitrary table port.

All argument cells are collected before folding, matching Python's aggregate
argument evaluation. A later access failure therefore precedes an earlier
potential overflow or type failure. SUM starts at the first non-missing value
and applies scalar addition in record order; MEAN divides this result by the
non-missing count. Types are checked as the fold reaches each present value,
including after a float overflow has normalized the running total to missing.
There is no group-wide float promotion or reassociation. Empty/all-missing groups
remain missing. The shared fixture has 32 independently specified outcomes,
including exact float bits, signed zero, integer overflow and failure precedence;
both Rust and the real Python aggregate evaluator replay it.

These are internal APIs, with fake-table application tests. Aggregate grammar,
computed arguments, grouping/filter evaluation and specification execution are
still outstanding. No dependencies,
host API or backend defaults change. The next storage adapter must validate
schema/value agreement at ingestion and preserve temporal collected precision;
a native Arrow date/timestamp array alone cannot carry that per-value metadata.

## Arrow snapshot storage

`yamaa-adapters::arrow_table::ArrowTable` implements `TableAccess` using owned,
immutable Arrow record batches. Construction checks caller-supplied column,
batch, total-row and logical-cell limits before validating schema or scanning
values. These are internal shape/work budgets, not a byte limit on buffers the
caller already allocated, nor a guarantee of recoverable allocation failure
inside Arrow. Installed IPC/request APIs must impose their own trusted limits.

The ordered schema and every chunk, including empty chunks, are retained. Empty
tables can have a schema without batches; zero-column batches retain their row
counts. Row lookup uses cumulative chunk ends and never computes partial sums.
The actual engine SUM/MEAN consumer is tested across chunk boundaries, including
rounding-sensitive cancellation and integer overflow. There are no per-cell
host calls or Arrow aggregate kernels.

The internal physical representation is deliberately closed:

| Logical column | Arrow representation |
| --- | --- |
| str | nullable Utf8 |
| int | nullable Int64; missing is validity, never an integer sentinel |
| float | nullable Float64; nonfinite inputs become actual nulls at construction |
| date | nullable Struct(value: non-null Date32, precision: non-null UInt8) |
| datetime | nullable Struct(value: non-null Timestamp(Second, no timezone), precision: non-null UInt8) |

Date precision codes are year=0, month=1 and day=2; datetime codes are day=0 and
second=1. The parent validity denotes missing and masks child payloads. Visible
children must exist, use a valid precision code, and name a civil value within
years 1..9999. The native value retains all imputed fields, independently of
collected precision. Timezones and other timestamp units are rejected. A plain
Arrow date/timestamp is not silently accepted as an internal precision-bearing
column; a future artifact ingestion adapter must explicitly assign full precision
at that boundary. Field order, names, nullability and metadata must match the
canonical schema exactly. This representation is internal, not yet a public IPC
protocol or Polars/R round-trip capability.

Construction narrows every admitted array to concrete immutable Arrow types,
retains their buffer ownership, and validates all visible temporal data before
execution. Finite arrays share buffers; float columns containing nonfinite values
are rebuilt with null validity. Read-only batch access returns normalized storage.
Tests cover input-handle release, UTF-8/NUL/empty text, full i64, subnormals/signed
zero, sliced arrays/structs, null parent/child masks, range/schema errors, zero
shapes, and independent epoch anchors plus a complete 146,097-day Gregorian cycle.
No zero-copy host FFI or untrusted-decoder safety is claimed.

Pins arrow-array, arrow-schema and arrow-buffer 60.0.0 with default features
disabled in adapters only (Apache-2.0 AND MIT, MSRV 1.88; toolchain 1.90).
[The upstream release](https://arrow.apache.org/blog/2026/09/29/arrow-rs-60.0.0/)
documents this MSRV. Split Arrow crates are rejected in core/engine by the
dependency guard. The transitive graph includes arrow-data, chrono with its clock
and platform support, ahash/hashbrown, half, num-complex, num-integer and num-traits;
disabling Arrow defaults does not remove those dependencies. This adds no IPC,
Parquet, compute-kernel or unsafe FFI API. Transitive Cargo locking remains an
unresolved release gate; no lockfile or content digest is committed.

## Installed table interchange

Python bytes and R raw vectors now cross the shared bounded Arrow IPC adapter.
Public exports sanitize masked payloads; exact inspection and installed Polars
restoration retain values and temporal precision. See
[TABLE_TRANSPORT.md](TABLE_TRANSPORT.md) for the closed schema, limits, error
categories, qualification evidence and remaining gates. This does not enable
specification execution or change backend defaults.

The trusted internal [function invocation service](FUNCTION_INVOCATION.md) now
validates exact signatures/defaults, preserves missing short-circuit and callback
order, and checks results before conversion. A synchronous already-bound port
keeps host errors opaque and results owned. Independent truth runs through Rust
and the real Python reference; installed Python/R adapters now call this service.
The optional Python project frontend now runs activation vectors through this
invoker over shared verified artifact bindings. Environment loading and vector
comparison remain host ports; full shared environment activation remains pending.

The installed Python [function/1 callback API](FUNCTION_TRANSPORT.md) now admits
bounded normalized requests and calls an explicit Python callable on the current
interpreter thread. It preserves exact scalars and portable fatal outcomes.
The installed R API invokes an explicit function on the R thread through the
same service; production environment binding and dataset execution remain pending.

The R source package now exposes [lossless int/str scalars](R_SCALARS.md),
with validated raw-byte native transport, checked shared arithmetic, exact host
conversions and NUL-preserving text access. Installed callbacks use those scalars
plus Date and explicitly UTC POSIXct, with bounded condition details, exact
result admission, ownership, cancellation and reentrancy tests.

The experimental [decoded schema service](SCHEMA_TRANSPORT.md) now interprets
shared schema bundles, document values, defaults and shorthand through the same
core in both installed hosts. Its prepared Python snapshot and stateless R batch
API do not invoke a host schema interpreter. Python's explicit
`yamaa.adapters.native_specification` loader uses the shared service for schema
admission, document normalization, [layer admission and composition](LAYER_COMPOSITION.md),
[shared named-window expansion](WINDOW_EXPANSION.md), and
[inheritance dependency resolution](INHERITANCE_DEPENDENCIES.md), while
using [shared parent traversal](INHERITANCE_TRAVERSAL.md) with explicit host source
ports and retaining final host modeling. Full current-schema R workflow
integration and release qualification remain open; no backend default or
execution-readiness flag changes.

The [shared YAML decoder](YAML_DECODING.md) provides byte-oriented `yaml/1`
entry points in both installed hosts. The optional Python loader captures it
before IO and uses it for schema, entry and inherited sources. Exact integer
identity and Unicode diagnostics survive the source boundary; host filesystem
authority and final model responsibilities remain explicit.

Shared [inheritance dependency resolution](INHERITANCE_DEPENDENCIES.md) now backs
opt-in Python pruning and ordering and the installed R schema query. This does
not enable qualified key-matched aggregate execution or complete R workflows.
