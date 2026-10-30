# Installing in a restricted environment

A machine without internet access, such as a validated server, installs yamaa
from an internal mirror, an internal PyPI index or a CRAN mirror such as Posit
Package Manager, or from files carried in, and needs no further network. This
page covers yamaa 0.6.0 and later releases.

Once installed, yamaa never reaches the network. It reads and writes local
files only, since a path in a specification has no URI scheme
([FILE-5](../../rules/reference/files.md#file-5)), and it opens no network
connection: no downloads, telemetry or update checks.

## Routes

Every route below needs no network once its files are on the machine. Each row
names what qualifies it: CRAN's binary builders, or jobs of the native
qualification workflow, `.github/workflows/native.yml`, every one of which
must pass in the native run that a release is published from
([Publication](../release.md#publication)). In a job's name, `<os>` is the
runner: `ubuntu-24.04` is Linux x86_64, `ubuntu-24.04-arm` Linux aarch64,
`macos-15` macOS arm64 and `windows-2025` Windows x64.

| Route | The machine needs | Qualified by |
| --- | --- | --- |
| Python wheel, from PyPI, a mirror or the release assets | CPython 3.12 or later on Linux x86_64 or aarch64 with glibc 2.28 or later, on macOS arm64 or on Windows x64 | `native-qualify (<os>, <python>, wheel)` on every platform with CPython 3.12 to 3.15; `native-glibc (<os>)` on AlmaLinux 8, whose glibc is 2.28, for both Linux architectures; and, for a wheel [carried in](#from-files-carried-in) with its dependencies, `native-offline-wheel (<os>, <python>)` on every platform with CPython 3.12 and 3.15, which installs it and its locked dependencies from one directory with no network and proves that installation with its selftest, while the conformance corpus runs on the same wheel in `native-qualify` and `native-glibc` |
| Python source archive, from PyPI, a mirror or the release assets, built with no network | CPython 3.12 or later, Cargo and rustc 1.82 or later and a C compiler, with maturin 1.9.6 and yamaa's dependencies installed first | `native-offline-source (<os>, python)` on every platform |
| R source package, from the release assets, built with no network | R 4.2.0 or later, Cargo and rustc 1.82 or later, a C compiler and GNU make; on Windows, the Rtools version for your R series (Rtools42 for R 4.2) and Rust's `x86_64-pc-windows-gnu` target | `native-offline-source (<os>, r)` on every platform with R 4.6.1, and `native-cran (<os>, R 4.2.0)`, which builds it with Rust 1.82 behind a proxy that refuses every connection |
| R binary package, from the release assets | R-release or R-oldrel, today R 4.6 or R 4.5, on Windows x64 or on macOS arm64 from macOS 11; no Rust or Rtools | `native-r-binary-install (<os>, R <version>)` on Windows x64 and macOS arm64 with R 4.6.1 and R 4.5.3 |
| [Offline Rust toolchain route](#the-offline-rust-toolchain-route), for either source archive on Linux | Linux x86_64 or aarch64, Rust's standalone installer carried in, and what the source archive needs besides Rust | `native-offline-toolchain (<os>, python)` and `native-offline-toolchain (<os>, r)` on both Linux architectures, with Rust 1.82.0 |
| R from CRAN or a CRAN mirror, from 0.6.0 | R 4.2.0 or later; on Windows and macOS, CRAN's binary package for R-release or R-oldrel; elsewhere, what the R source package needs | CRAN's binary builders, from the release's R source package, on which `native-cran` runs CRAN's incoming checks |
| Command-line binary, from the release assets | Linux x86_64 or aarch64, whatever its C library, since the binary is static; Windows x64; or macOS arm64 from macOS 11; no Python, R or Rust | `native-cli-install (<os>, <target>)` on every platform, which unpacks the archive that `native-cli (<os>, <target>)` built and runs `yamaa selftest` and the conformance corpus from it, with no network |

The floors are CPython 3.12 (#3030 D56, confirming D13) and R 4.2.0. A source
build needs Rust 1.82, the minimum that the Rust workspace declares; a
repository check refuses a locked crate that needs a newer Rust.
[Supported platforms](../release.md#supported-platforms) lists the platforms
in full.

## From an internal mirror

### An internal PyPI index

From 0.5.0 on, each release's wheels and Python source archive are on
[PyPI](https://pypi.org/project/yamaa/). An index that proxies
PyPI, such as an Artifactory PyPI repository, fetches them and yamaa's
dependencies on the first request and serves them from then on, so pip on the
restricted machine needs only the index:

```sh
python -m pip install --index-url <index URL> yamaa
```

A release candidate is a pre-release, which pip installs only when asked: name
it with `--pre`, as in `python -m pip install --pre yamaa==<candidate>`.

An index that does not proxy PyPI can host the files instead: upload the
wheels and the source archive from the release assets, which are the files
that PyPI serves. yamaa has no dependencies; add Polars, pandas or PyArrow if
you convert its tables to theirs.

### A CRAN mirror

CRAN receives final releases from 0.6.0 on, never a candidate (#3673 D289),
except 0.7.0, which is not submitted (#4064 D304). Once CRAN has published one, a CRAN mirror serves yamaa as it serves any CRAN
package: Posit Package Manager's CRAN repository follows CRAN, so
`install.packages("yamaa")` with the mirror as the repository installs it,
and a curated repository serves it once an administrator adds it. On Windows and macOS, CRAN provides binary
packages for R-release and R-oldrel; elsewhere the source package builds with
no network, as [the next section](#from-files-carried-in) describes.

Before then, for 0.7.0 and for every candidate, the R packages are release
assets only:
add them to a local repository that the mirror hosts, or carry them in.

## From files carried in

When no mirror has the version you need, the assets of its
[GitHub release](https://github.com/elong0527/yamaa/releases) are the
fallback: the wheels, the Python source archive, the R source package, the R
binary packages and the command-line archives. The wheels and the Python
source archive are the files that PyPI serves, a final release's R source
package is the file that CRAN receives (0.7.0's goes nowhere else), and the R
binary packages and the
command-line archives are published only there. Download them on a connected
machine and carry them in; installing them needs no network.

A wheel needs its dependencies beside it. On a connected machine with the same
operating system, architecture and Python version, collect yamaa and its
dependencies into one directory, adding `--pre` for a candidate, then install
from that directory alone on the restricted machine:

```sh
python -m pip download --dest yamaa-files yamaa==<version>
python -m pip install --no-index --find-links yamaa-files yamaa
```

An R binary package needs no other file, since the package imports only
`utils`, which ships with R. Install the one for your platform and R series,
which ends its file name:

```r
install.packages("yamaa_<version>-4.6.zip", repos = NULL)  # Windows x64, R 4.6
install.packages("yamaa_<version>-4.6.tgz", repos = NULL)  # macOS arm64, R 4.6
```

A command-line archive needs no other file and no installer. Unpack the one
for your platform, whose name ends with its Rust target, such as
`yamaa-<version>-x86_64-unknown-linux-musl.zip`, and run the `yamaa`
executable in the directory that it holds, `yamaa.exe` on Windows. The
binaries are unsigned: [Command-line binaries](../release.md#command-line-binaries)
says what macOS and Windows show the first time, and how to go on.

A source archive carries every Rust crate that it builds, and Cargo reads no
other, so the build needs a Rust toolchain but no network. The R source
package's `configure` looks for Cargo and rustc on the PATH and then in
`~/.cargo/bin`, reports both versions in the installation log, and stops with
advice when either is missing or older than 1.82:

```r
install.packages("yamaa_<version>.tar.gz", repos = NULL, type = "source")
```

pip's build isolation would download maturin to build the Python source
archive, so install maturin 1.9.6 first and build without isolation. The
first command runs on the connected machine, the other two on the restricted
one:

```sh
python -m pip download --dest yamaa-files maturin==1.9.6
python -m pip install --no-index --find-links yamaa-files maturin==1.9.6
python -m pip install --no-build-isolation --no-index --find-links yamaa-files \
  yamaa-<version>.tar.gz
```

## The offline Rust toolchain route

A Linux machine whose packaged Rust is older than 1.82 can still build either
source archive: carry in Rust's standalone installer and install it with no
rustup and no network (#3030 D54). Native qualification runs these steps on
Linux x86_64 and aarch64 with Rust 1.82.0, the minimum, which the commands
name.

1. On a connected machine, download from static.rust-lang.org the installer
    archive for the restricted machine's target, its signature and the Rust
    signing key. From this repository, download the reviewed copy of that key
    that native qualification verifies the installer against,
    [`tools/ci/keys/rust-key.gpg.ascii`](../../tools/ci/keys/rust-key.gpg.ascii)
    (#3030 D135), as `yamaa-rust-key.gpg.ascii`, from the tag of the release
    that you install: `yamaa-v<version>`, such as `yamaa-v0.6.0`, or
    `yamaa-v<version>-rc.<N>` for a candidate. The target is
    `x86_64-unknown-linux-gnu` or `aarch64-unknown-linux-gnu`: `uname -m` on
    the restricted machine prints its first part.

    ```sh
    target=x86_64-unknown-linux-gnu
    installer="rust-1.82.0-${target}.tar.xz"
    curl --proto '=https' --tlsv1.2 -fLO "https://static.rust-lang.org/dist/${installer}"
    curl --proto '=https' --tlsv1.2 -fLO "https://static.rust-lang.org/dist/${installer}.asc"
    curl --proto '=https' --tlsv1.2 -fLO https://static.rust-lang.org/rust-key.gpg.ascii
    curl --proto '=https' --tlsv1.2 -fL -o yamaa-rust-key.gpg.ascii \
      "https://raw.githubusercontent.com/elong0527/yamaa/yamaa-v<version>/tools/ci/keys/rust-key.gpg.ascii"
    ```

2. The Rust project signs its
    [standalone installers](https://forge.rust-lang.org/infra/other-installation-methods.html#standalone-installers)
    with the Rust signing key. `cmp` compares the two copies of the key byte
    for byte and prints nothing when they are identical. They come from two
    hosts, so a forged key served by either one shows up as a difference. Only
    when they match do the commands go on: they import the reviewed copy,
    `yamaa-rust-key.gpg.ascii`, into a keyring of its own, and GnuPG checks
    the signature there, with automatic key retrieval off. If `cmp` reports a
    difference, stop, and use neither the key nor the archive. Use the archive
    only if gpg reports a good signature from "Rust Language (Tag and Release
    Signing Key)".

    ```sh
    export GNUPGHOME="$PWD/gnupg"
    mkdir -m 700 "$GNUPGHOME"
    cmp rust-key.gpg.ascii yamaa-rust-key.gpg.ascii &&
      gpg --batch --import yamaa-rust-key.gpg.ascii &&
      gpg --batch --no-auto-key-retrieve --verify "${installer}.asc" "${installer}"
    ```

3. Carry the archive in, unpack it and run its own `install.sh`. It installs
    rustc, Cargo and the standard library of the target, the components of
    rustup's minimal profile, into the prefix, here `/opt/rust`; any directory
    that you can write to will do. Where the machine has no `xz`, unpack the
    archive on the connected machine and carry in the directory.

    ```sh
    tar -xJf "${installer}"
    "./rust-1.82.0-${target}/install.sh" --prefix=/opt/rust \
      --components="rustc,cargo,rust-std-${target}" --disable-ldconfig
    /opt/rust/bin/rustc --version
    ```

4. Put the prefix's `bin` directory first on the PATH and build the source
    archive as [above](#from-files-carried-in), with no network. For an R
    session, such as one in RStudio, add `PATH=/opt/rust/bin:${PATH}` to
    `~/.Renviron` instead.

    ```sh
    export PATH="/opt/rust/bin:$PATH"
    R CMD INSTALL yamaa_<version>.tar.gz
    python -m pip install --no-build-isolation --no-index --find-links yamaa-files \
      yamaa-<version>.tar.gz
    ```

On Windows and macOS, the wheels and the R binary packages need no Rust; this
route is qualified on Linux only.

## Check the installation

Run the selftest on the restricted machine after installing. It runs the cases
that the package carries through the installed engine and compares each result
with its independently authored expected files, writing no file and using no
network. Keep its printed report as the installation and operational
qualification record of that machine. The `native-offline-wheel`,
`native-offline-source`, `native-offline-toolchain` and
`native-r-binary-install` jobs run the same selftest after installing with
no network, and fail unless it reports `succeeded`; the `native-cli-install`
jobs unpack each command-line archive and run `yamaa selftest` from it with
no network, and fail unless it succeeds.
[Check an installation](api.md#check-an-installation) describes the result.

=== "R"

    ```r
    yamaa_selftest()
    ```

=== "Python"

    ```python
    import yamaa
    print(yamaa.selftest())
    ```

=== "Command line"

    ```sh
    yamaa selftest
    ```

## Proxies

Once the files are on the machine, there is no proxy to configure:
`pip install --no-index` reads only the files that it is given,
`install.packages(repos = NULL)` installs a local file, and each source
archive builds from the crates that it carries.
