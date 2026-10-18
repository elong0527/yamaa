# yamaa for Python <img src="https://raw.githubusercontent.com/elong0527/yamaa/main/docs/assets/logo.jpeg" align="right" width="120" alt="yamaa logo" />

[![CI](https://github.com/elong0527/yamaa/actions/workflows/ci.yml/badge.svg?branch=main&event=schedule)](https://github.com/elong0527/yamaa/actions/workflows/ci.yml)
[![Regular build](https://img.shields.io/github/actions/workflow/status/elong0527/yamaa/native.yml?branch=main&event=schedule&label=regular%20build)](https://github.com/elong0527/yamaa/actions/workflows/native.yml)
[![Coverage](https://codecov.io/gh/elong0527/yamaa/graph/badge.svg)](https://codecov.io/gh/elong0527/yamaa)
[![Docs](https://github.com/elong0527/yamaa/actions/workflows/docs.yml/badge.svg)](https://elong0527.github.io/yamaa/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/elong0527/yamaa/blob/main/LICENSE)

Python package for yamaa, a domain-specific language for clinical trial data
standardization.

Runs yamaa specifications that transform ODM XML data into SDTM and ADaM
datasets, and generates define.xml, with the same Rust engine as the
[R package](https://github.com/elong0527/yamaa/tree/main/R/yamaa). See the
[Python package documentation](https://elong0527.github.io/yamaa/articles/python/)
for usage.

## Installation

Download the wheel for your platform from
[GitHub releases](https://github.com/elong0527/yamaa/releases), then install it:

```bash
python -m pip install ./yamaa-0.4.0-<wheel-tags>.whl
```

Or install the standalone source archive from the same release. Building it
needs Rust 1.90.0 with Cargo, a C compiler and GNU make:

```bash
python -m pip install ./yamaa-0.4.0.tar.gz
```

To set up the development version from GitHub, install uv, the pinned Rust
toolchain and a C compiler, then run:

```bash
git clone https://github.com/elong0527/yamaa.git
cd yamaa
make setup
```

yamaa 0.4.0 is published as GitHub release assets, not on PyPI. The
[release guidance](https://elong0527.github.io/yamaa/release/#supported-platforms)
lists the supported Python versions and platforms.

## Contributing

We welcome contributions to yamaa. Please read the
[contributing guide](https://github.com/elong0527/yamaa/blob/main/CONTRIBUTING.md)
to get started.

## License

This project is licensed under the terms of the MIT license. The package also
bundles third-party components, whose license notices ship as its
`LICENSE-*.txt` files.
