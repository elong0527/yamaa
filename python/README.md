# yamaa for Python <img src="https://raw.githubusercontent.com/elong0527/yamaa/yamaa-v0.6.1/docs/assets/logo.jpeg" align="right" width="120" alt="yamaa logo" />

[![CI](https://github.com/elong0527/yamaa/actions/workflows/ci.yml/badge.svg?branch=main&event=schedule)](https://github.com/elong0527/yamaa/actions/workflows/ci.yml)
[![Regular build](https://img.shields.io/github/actions/workflow/status/elong0527/yamaa/native.yml?branch=main&event=schedule&label=regular%20build)](https://github.com/elong0527/yamaa/actions/workflows/native.yml)
[![Coverage](https://codecov.io/gh/elong0527/yamaa/graph/badge.svg)](https://codecov.io/gh/elong0527/yamaa)
[![Docs](https://github.com/elong0527/yamaa/actions/workflows/docs.yml/badge.svg)](https://elong0527.github.io/yamaa/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/elong0527/yamaa/blob/yamaa-v0.6.1/LICENSE)

Python package for yamaa, a domain-specific language for clinical trial data
standardization.

Runs yamaa specifications that transform ODM XML data into SDTM and ADaM
datasets, and generates define.xml, with the same Rust engine as the
[R package](https://github.com/elong0527/yamaa/tree/yamaa-v0.6.1/R/yamaa):

```python
import yamaa

project = yamaa.project("project.yaml")
run = project.spec("DM").run()      # one dataset, built in memory
run.output, run.issues, run.report
run.status                          # "succeeded" or "failed"
dm = run.output.to_polars()         # or to_pandas(), to_arrow(), to_pylist()
paths = run.save()                  # write the dataset and its run report
project.run("adam").save()          # every ADaM dataset
```

Every table is a `yamaa.Table`, which needs no other package: it prints a
preview, `num_rows` and `column_names` describe it, and `to_pylist()` gives
its rows. `to_polars()`, `to_pandas()` and `to_arrow()` convert it when
Polars, pandas or PyArrow is installed. `save()` returns the absolute paths of
the files it wrote, in the order it wrote them.

The
[R, Python and command-line API](https://elong0527.github.io/yamaa/articles/api/)
page describes every call, and the
[introduction](https://elong0527.github.io/yamaa/articles/intro/) explains the
specification language.

## Installation

From 0.5.0 on, yamaa is published on
[PyPI](https://pypi.org/project/yamaa/). Install the newest release with:

```bash
python -m pip install yamaa
```

A release candidate is a pre-release there, which pip installs only when
asked, so name it with `--pre`, as in
`python -m pip install --pre yamaa==<candidate>`.

The wheels cover CPython 3.12 and later on Linux x86_64 and aarch64, macOS
arm64 and Windows x64. The Linux wheels are `manylinux_2_28` and need glibc
2.28 or later, for example RHEL 8, Ubuntu 20.04 or Debian 10; elsewhere, pip
builds the source archive, as described below.

Every release is also published as assets of its
[GitHub release](https://github.com/elong0527/yamaa/releases): from 0.5.0 on
the same files that PyPI serves, and yamaa 0.4.0 only there. Download the
wheel for your platform, then install it:

```bash
python -m pip install ./yamaa-<version>-<wheel-tags>.whl
```

The 0.4.0 Linux wheel is `manylinux_2_34` and needs glibc 2.34 or later.

Or install the standalone source archive from the same release. Building it
uses your default Rust toolchain, which must be Rust 1.90 or later for 0.4.0,
and Rust 1.82 or later from 0.5.0 on, with Cargo, a C compiler and GNU make:

```bash
python -m pip install ./yamaa-<version>.tar.gz
```

From 0.6.0 on, the source archive carries the locked
Rust crates, so the build reads nothing from crates.io; pip's build isolation
still fetches `maturin`.
To build offline, install `maturin` 1.9.6 and pass `--no-build-isolation`. The
0.4.0 and 0.5.0 source archives download the locked crates from crates.io, so
building them offline also needs the Cargo cache filled first.

On a machine without internet access, install from an internal package index
or from files carried in, as
[Installing in a restricted environment](https://elong0527.github.io/yamaa/articles/restricted-environments/)
describes. It also shows how a Linux machine whose packaged Rust is older than
1.82 installs Rust from its standalone installer to build the source archive.

To set up the development version from GitHub, install uv, the pinned Rust
toolchain and a C compiler, then run:

```bash
git clone https://github.com/elong0527/yamaa.git
cd yamaa
make setup
```

The package has no runtime dependencies. The `test` extra adds Polars,
pandas, PyArrow, the Python YAML readers, lxml and Pydantic for the table
conversions' tests and independent artifact and conformance checks.
`make setup` installs it.

The [release guidance](https://elong0527.github.io/yamaa/release/#supported-platforms)
lists the supported Python versions and platforms.

## Contributing

We welcome contributions to yamaa. Please read the
[contributing guide](https://github.com/elong0527/yamaa/blob/main/CONTRIBUTING.md)
to get started.

## License

This project is licensed under the terms of the MIT license. The package also
bundles third-party components, whose license notices ship as its
`LICENSE-*.txt` files.
