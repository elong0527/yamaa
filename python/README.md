# yamaa for Python <img src="https://raw.githubusercontent.com/elong0527/yamaa/yamaa-v0.5.0/docs/assets/logo.jpeg" align="right" width="120" alt="yamaa logo" />

[![CI](https://github.com/elong0527/yamaa/actions/workflows/ci.yml/badge.svg?branch=main&event=schedule)](https://github.com/elong0527/yamaa/actions/workflows/ci.yml)
[![Regular build](https://img.shields.io/github/actions/workflow/status/elong0527/yamaa/native.yml?branch=main&event=schedule&label=regular%20build)](https://github.com/elong0527/yamaa/actions/workflows/native.yml)
[![Coverage](https://codecov.io/gh/elong0527/yamaa/graph/badge.svg)](https://codecov.io/gh/elong0527/yamaa)
[![Docs](https://github.com/elong0527/yamaa/actions/workflows/docs.yml/badge.svg)](https://elong0527.github.io/yamaa/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/elong0527/yamaa/blob/yamaa-v0.5.0/LICENSE)

Python package for yamaa, a domain-specific language for clinical trial data
standardization.

Runs yamaa specifications that transform ODM XML data into SDTM and ADaM
datasets, and generates define.xml, with the same Rust engine as the
[R package](https://github.com/elong0527/yamaa/tree/yamaa-v0.5.0/R/yamaa):

```python
import yamaa

project = yamaa.project("project.yaml")
run = project.spec("DM").run()      # one dataset, built in memory
run.output, run.issues, run.report
run.save()                          # write the dataset and its run report
project.run("adam").save()          # every ADaM dataset
```

The [R and Python API](https://elong0527.github.io/yamaa/articles/api/) page
describes every call, and the
[introduction](https://elong0527.github.io/yamaa/articles/intro/) explains the
specification language.

## Installation

From its 1.0.0 release candidates on, yamaa is published on
[PyPI](https://pypi.org/project/yamaa/). A candidate is a pre-release there, so
name it, for example `python -m pip install --pre yamaa==1.0.0rc1`. Once 1.0.0
is out, install the newest release with:

```bash
python -m pip install yamaa
```

The wheels cover CPython 3.12 and later on Linux x86_64 and aarch64, macOS
arm64 and Windows x64. The Linux wheels are `manylinux_2_28` and need glibc
2.28 or later, for example RHEL 8, Ubuntu 20.04 or Debian 10; elsewhere, pip
builds the source archive, as described below.

Every release is also published as assets of its
[GitHub release](https://github.com/elong0527/yamaa/releases), the same files
that PyPI serves, and yamaa 0.4.0 and 0.5.0 only there. Download the wheel for
your platform, then install it:

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

From 1.0.0 on, the source archive carries the locked Rust crates, so the build
reads nothing from crates.io; pip's build isolation still fetches `maturin`.
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

The development package requires only Polars and packaging at runtime. The
`test` extra adds the Python YAML readers, PyArrow, lxml and Pydantic for
independent artifact and conformance checks. `make setup` installs it.

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
