"""`make test-r` shares one engine build between its two installations in CI.

CI's r job points YAMAA_R_CARGO_TARGET_DIR at the directory that its Rust
cache restores, and `make test-r` then refuses an installation in R CMD check
that compiles the engine again.
"""

import contextlib
import io
import unittest

import yaml
from tools.commands import r_tests
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader

INSTALL_LOG = """\
using rustc 1.99.1 (58cf8f9c3 2026-10-13) from /usr/bin/rustc
Cargo builds in /work/rust/target, which YAMAA_R_CARGO_TARGET_DIR names
gcc -I"/usr/share/R/include" -DNDEBUG -fpic -O2 -c entrypoint.c -o entrypoint.o
{cargo}    Finished `release` profile [optimized] target(s) in {time}
* DONE (yamaa)
"""


def load(relative):
    return yaml.load(
        (ROOT / relative).read_text(encoding="utf-8"), Loader=UniqueKeyLoader
    )


class SharedBuild(unittest.TestCase):
    def test_the_r_job_builds_in_the_directory_that_its_rust_cache_restores(self):
        steps = load(".github/actions/setup/action.yml")["runs"]["steps"]
        (cache,) = [
            step
            for step in steps
            if step.get("uses", "").startswith("Swatinem/rust-cache@")
        ]
        workspace, _, target = cache["with"]["workspaces"].partition("->")
        directory = f"{workspace.strip()}/{target.strip() or 'target'}"
        steps = load(".github/workflows/ci.yml")["jobs"]["r"]["steps"]
        (test,) = [
            step
            for step in steps
            if "tools.commands.host_checks" in step.get("run", "")
        ]
        self.assertEqual(
            test.get("env"),
            {
                "EQUIVALENCE": "${{ steps.paths.outputs.equivalence_r }}",
                "YAMAA_R_CARGO_TARGET_DIR": "${{ github.workspace }}/" + directory,
            },
        )

    def test_an_installation_that_finds_the_build_complete_passes(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            r_tests.check_shared_build(INSTALL_LOG.format(cargo="", time="0.41s"))
        self.assertIn("reused the engine build", output.getvalue())

    def test_an_installation_that_compiles_again_is_refused(self):
        cargo = (
            "   Compiling yamaa-adapters v0.5.0 (/work/yamaa/src/rust/crates/yamaa-adapters)\n"
            "   Compiling yamaa-r v0.5.0 (/work/yamaa/src/rust/crates/yamaa-r)\n"
        )
        with self.assertRaisesRegex(
            RuntimeError, "compiled yamaa-adapters, yamaa-r again instead of reusing"
        ):
            r_tests.check_shared_build(INSTALL_LOG.format(cargo=cargo, time="1m 52s"))


if __name__ == "__main__":
    unittest.main()
