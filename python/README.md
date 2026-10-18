# yamaa for Python <img src="https://raw.githubusercontent.com/elong0527/yamaa/yamaa-v0.4.0/docs/assets/logo.jpeg" align="right" width="120" alt="yamaa logo" />

[![CI](https://github.com/elong0527/yamaa/actions/workflows/ci.yml/badge.svg?branch=main&event=schedule)](https://github.com/elong0527/yamaa/actions/workflows/ci.yml)
[![Regular build](https://img.shields.io/github/actions/workflow/status/elong0527/yamaa/native.yml?branch=main&event=schedule&label=regular%20build)](https://github.com/elong0527/yamaa/actions/workflows/native.yml)
[![Coverage](https://codecov.io/gh/elong0527/yamaa/graph/badge.svg)](https://codecov.io/gh/elong0527/yamaa)
[![Docs](https://github.com/elong0527/yamaa/actions/workflows/docs.yml/badge.svg)](https://elong0527.github.io/yamaa/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/elong0527/yamaa/blob/yamaa-v0.4.0/LICENSE)

Python package for yamaa, a domain-specific language for clinical trial data
standardization.

Runs yamaa specifications that transform ODM XML data into SDTM and ADaM
datasets, and generates define.xml, with the same Rust engine as the
[R package](https://github.com/elong0527/yamaa/tree/yamaa-v0.4.0/R/yamaa). See the
[Python package documentation](https://github.com/elong0527/yamaa/blob/yamaa-v0.4.0/docs/articles/python.md)
for usage.

## Installation

Download the wheel for your platform from the
[GitHub release](https://github.com/elong0527/yamaa/releases/tag/yamaa-v0.4.0),
then install it:

```bash
python -m pip install ./yamaa-0.4.0-<wheel-tags>.whl
```

The Linux wheel is `manylinux_2_34` and needs glibc 2.34 or later, for example
RHEL 9, Ubuntu 22.04 or Debian 12; on older systems, install the source archive.

Or install the standalone source archive from the same release. Building it
needs Rust 1.90.0 with Cargo, a C compiler and GNU make:

```bash
python -m pip install ./yamaa-0.4.0.tar.gz
```

The build downloads the locked Rust crates from crates.io, and pip's build
isolation fetches `maturin`. To build offline, fill the Cargo cache first,
install `maturin` 1.9.6 and pass `--no-build-isolation`.

To set up the development version from GitHub, install uv, the pinned Rust
toolchain and a C compiler, then run:

```bash
git clone https://github.com/elong0527/yamaa.git
cd yamaa
make setup
```

yamaa 0.4.0 is published as GitHub release assets, not on PyPI. The
[release guidance](https://github.com/elong0527/yamaa/blob/yamaa-v0.4.0/docs/release.md#supported-platforms)
lists the supported Python versions and platforms.

## Contributing

We welcome contributions to yamaa. Please read the
[contributing guide](https://github.com/elong0527/yamaa/blob/main/CONTRIBUTING.md)
to get started.

## License

This project is licensed under the terms of the MIT license. The package also
bundles third-party components, whose license notices ship as its
`LICENSE-*.txt` files.
