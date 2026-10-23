"""The test-only loaded mode of both hosts (rust/interface/README.md).

A binding built with its `dynamic-engine` feature links no engine: at its
handshake it loads the engine library from beside its own module file. This
module builds that library and the loading tests' fixture engines, places the
library beside an installed Python package's module, and patches a staged R
package, never the repository's own build files, so that it builds both its
binding and the engine library that way. A repository check refuses the
feature in every build that ships (tools/validation/check_loaded_mode.py).

YAMAA_ENGINE selects the link mode of `make setup`, `make test-python`,
`make test-r` and CI's host checks: `linked`, the default, which every
shipped package uses, or `loaded`. In the loaded mode the host suites find
the fixture engines in the directory that YAMAA_FIXTURE_ENGINES names.
"""

import json
import os
import shutil
import subprocess
from pathlib import Path

from tools.paths import WORKSPACE

VARIABLE = "YAMAA_ENGINE"
FIXTURES_VARIABLE = "YAMAA_FIXTURE_ENGINES"
FEATURE = "dynamic-engine"
# The file name the loader looks for beside a binding's module
# (rust/crates/yamaa-boundary/src/loader.rs).
LIBRARY = "yamaa_engine.dll" if os.name == "nt" else "libyamaa_engine.so"
# The fixture engines of rust/crates/yamaa-engine/examples, which the shared
# truth's interface family names.
FIXTURES = (
    "fixture_next_version",
    "fixture_no_entry",
    "fixture_no_plan_reuse",
    "fixture_no_format_parquet",
)


def mode():
    """The link mode YAMAA_ENGINE selects: linked, or loaded."""
    value = os.environ.get(VARIABLE, "linked")
    if value not in ("linked", "loaded"):
        raise ValueError(f"{VARIABLE} must be linked or loaded, not {value!r}")
    return value


def loaded():
    return mode() == "loaded"


def built(arguments):
    """Run a Cargo build in the workspace; return each target's shared library."""
    completed = subprocess.run(
        ["cargo", *arguments, "--message-format=json-render-diagnostics"],
        cwd=WORKSPACE,
        stdout=subprocess.PIPE,
        text=True,
        check=True,
    )
    libraries = {}
    for line in completed.stdout.splitlines():
        message = json.loads(line)
        if message.get("reason") != "compiler-artifact":
            continue
        for name in message["filenames"]:
            if Path(name).suffix in (".so", ".dylib", ".dll"):
                libraries[message["target"]["name"]] = Path(name)
    return libraries


def build_engine(profile="dev"):
    """Build the loadable engine library; return its path."""
    libraries = built(
        [
            "rustc",
            "--locked",
            "-p",
            "yamaa-engine",
            "--lib",
            "--profile",
            profile,
            "--crate-type",
            "cdylib",
            "--features",
            FEATURE,
        ]
    )
    return libraries["yamaa_engine"]


def place(library, destination):
    """Put `library` at `destination`: a symbolic link where the platform
    allows one, else a copy."""
    destination.unlink(missing_ok=True)
    if os.name == "nt":
        shutil.copy2(library, destination)
    else:
        destination.symlink_to(library)


def build_fixtures(directory, profile="dev"):
    """Build the fixture engines into `directory`, each under its own name,
    and return the directory."""
    libraries = built(
        ["build", "--locked", "-p", "yamaa-engine", "--examples", "--profile", profile]
    )
    directory.mkdir(parents=True, exist_ok=True)
    for name in FIXTURES:
        place(libraries[name], directory / name)
    return directory


def package_directory(python):
    """The directory of the yamaa package that `python` imports, found
    without importing it, which would negotiate."""
    found = subprocess.check_output(
        [
            str(python),
            "-I",
            "-c",
            "import importlib.util; "
            "print(importlib.util.find_spec('yamaa').submodule_search_locations[0])",
        ],
        text=True,
    )
    return Path(found.strip())


def install_beside(python, library):
    """Place the engine library beside the native module of the yamaa
    package that `python` imports."""
    place(library, package_directory(python) / LIBRARY)


# The maturin arguments of a wheel in the loaded mode: the binding without
# its default linked engine, with the extension module pyproject.toml names.
WHEEL_ARGUMENTS = f"--no-default-features --features extension-module,{FEATURE}"

# The R build files' Cargo command for the binding, and for each the line
# that builds the loadable engine with the same toolchain and copies it into
# src/, where R installs every shared library beside yamaa's own.
R_BINDING = "-p yamaa-r $(ISOLATION_CONFIG)"
R_LOADED_BINDING = (
    f"-p yamaa-r --no-default-features --features {FEATURE} $(ISOLATION_CONFIG)"
)
R_ENGINE = {
    "Makevars.in": (
        "\tMAKEFLAGS= MFLAGS= CARGO_MAKEFLAGS= RUSTUP_AUTO_INSTALL=0 $(CARGO_ENV) "
        '$(ISOLATION_ENV) CARGO_HOME="$(CARGO_HOME_DIR)" '
        'CARGO_TARGET_DIR="$(CARGO_TARGET_DIR)" RUSTC="$(RUSTC)" CC="$(CC)" '
        '"$(CARGO)" rustc --locked --offline -j 2 --release --manifest-path '
        "rust/Cargo.toml -p yamaa-engine --lib --crate-type cdylib "
        f"--features {FEATURE} $(ISOLATION_CONFIG)\n"
        '\tfor library in "$(CARGO_TARGET_DIR)/release/libyamaa_engine.so" '
        '"$(CARGO_TARGET_DIR)/release/libyamaa_engine.dylib"; do '
        'if [ -f "$$library" ]; then cp "$$library" libyamaa_engine.so; fi; done\n'
    ),
    "Makevars.win.in": (
        "\tMAKEFLAGS= MFLAGS= CARGO_MAKEFLAGS= RUSTUP_AUTO_INSTALL=0 $(ISOLATION_ENV) "
        'CARGO_HOME="$(CARGO_HOME_DIR)" '
        'LIBRARY_PATH="$(LIBRARY_PATH);$(CARGO_TARGET_DIR)/libgcc_mock" '
        'CC_x86_64_pc_windows_gnu="$(CC)" AR_x86_64_pc_windows_gnu="$(AR)" '
        'CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$(CC)" '
        'CARGO_TARGET_DIR="$(CARGO_TARGET_DIR)" RUSTC="$(RUSTC)" '
        '"$(CARGO)" rustc --locked --offline -j 2 --release --target $(RUST_TARGET) '
        "--manifest-path rust/Cargo.toml -p yamaa-engine --lib --crate-type cdylib "
        f"--features {FEATURE} $(ISOLATION_CONFIG)\n"
        '\tcp "$(CARGO_TARGET_DIR)/$(RUST_TARGET)/release/yamaa_engine.dll" '
        "yamaa_engine.dll\n"
    ),
}


def patch_r_package(package):
    """Make the staged R package at `package` build its binding in the
    loaded mode and install the engine library beside it."""
    for name, engine in R_ENGINE.items():
        path = package / "src" / name
        lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
        [index] = [
            i for i, line in enumerate(lines) if line.rstrip().endswith(R_BINDING)
        ]
        lines[index] = lines[index].replace(R_BINDING, R_LOADED_BINDING)
        lines.insert(index + 1, engine)
        path.write_text("".join(lines), encoding="utf-8")
