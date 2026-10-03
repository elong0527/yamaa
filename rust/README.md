# Shared native engine bootstrap

This is the installation slice of [#1585](https://github.com/elong0527/yamaa/issues/1585).
It does not evaluate specifications. Both installed bindings call the same
`yamaa-engine`/`yamaa-core` code and report `execution_supported = false`.
The existing Python engine remains the default; the legacy R package is unchanged.

## Boundaries

| Crate | Responsibility | Allowed dependencies |
| --- | --- | --- |
| `yamaa-core` | Language contracts | Pinned `ryu` for no_std numeric formatting |
| `yamaa-engine` | Application entry points | Core |
| `yamaa-adapters` | Infrastructure, including embedded resources | Core, engine |
| `yamaa-python` | Python binding | Engine, adapters, PyO3 |
| `yamaa-r` | R binding | Engine, adapters, extendr |

Core and engine use `no_std` and forbid unsafe code. The dependency guard checks
all Cargo dependency kinds, including build, development, and target-specific
edges. New dependencies require a deliberate update to its allowlist. There are
no evaluation ports or fake application services yet: those belong with their
first actual use cases.

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

`evaluation::NumericPlan` evaluates the implemented numeric subset from a typed
tree through a core-owned resolver. It preserves operand order, association,
missing versus absent values, opaque resolution failures, and diagnostic
provenance. Shared evaluation fixtures compare values and resolution traces with
Python's real numeric parser/evaluator. The test-only postfix tree notation is
not an implementation of the numeric grammar. Parsing, compiler resource
budgets, the remaining functions, and lifecycle handlers are separate gates.

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
