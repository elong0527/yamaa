"""The Linux wheels build in the pinned manylinux_2_28 container of each machine."""

import contextlib
import io
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.ci import manylinux, release_assets, toolchain
from tools.paths import ROOT


class ManylinuxBuild(unittest.TestCase):
    def test_each_architecture_has_a_dated_manylinux_2_28_image(self):
        self.assertEqual(set(manylinux.IMAGES), {"x86_64", "aarch64"})
        tags = set()
        for architecture, image in manylinux.IMAGES.items():
            with self.subTest(architecture=architecture):
                name, tag = image.rsplit(":", 1)
                self.assertEqual(
                    name, f"quay.io/pypa/{manylinux.POLICY}_{architecture}"
                )
                # A dated tag, never a moving one such as latest.
                self.assertRegex(tag, r"[0-9]{4}\.[0-9]{2}\.[0-9]{2}-[0-9]+")
                tags.add(tag)
        self.assertEqual(len(tags), 1, "both images move together")

    def test_the_declared_linux_wheels_carry_the_policy_tag(self):
        self.assertEqual(
            sorted(
                tag
                for label, tag in release_assets.WHEEL_PLATFORMS.values()
                if label.startswith("Linux ")
            ),
            sorted(
                f"{manylinux.POLICY}_{architecture}"
                for architecture in manylinux.IMAGES
            ),
        )

    def test_the_container_builds_from_a_read_only_checkout(self):
        output = Path("/build/out")
        command = manylinux.command("docker", "aarch64", output, jobs=4)
        self.assertEqual(command[:3], ["docker", "run", "--rm"])
        volumes = [
            command[index + 1]
            for index, item in enumerate(command)
            if item == "--volume"
        ]
        self.assertEqual(volumes, [f"{ROOT}:/io:ro", f"{output}:/out"])
        environment = dict(
            command[index + 1].split("=", 1)
            for index, item in enumerate(command)
            if item == "--env"
        )
        self.assertEqual(
            environment,
            {
                "RUST_TOOLCHAIN": toolchain.channel(),
                "MATURIN": manylinux.maturin(),
                "POLICY": "manylinux_2_28",
                "PYTHON": "/opt/python/cp312-cp312/bin/python",
                "CARGO_BUILD_JOBS": "4",
            },
        )
        self.assertEqual(command[-4:-1], [manylinux.IMAGES["aarch64"], "bash", "-c"])
        script = command[-1]
        for text in (
            "CARGO_TARGET_DIR=/tmp/target",
            'rustup toolchain install "${RUST_TOOLCHAIN}"',
            '-m maturin build --release --locked --compatibility "${POLICY}"',
            "--out /out",
        ):
            with self.subTest(text=text):
                self.assertIn(text, script)
        self.assertNotIn(
            "CARGO_BUILD_JOBS", manylinux.command("docker", "x86_64", output)
        )

    def test_maturin_is_the_python_build_requirement(self):
        self.assertRegex(manylinux.maturin(), r"maturin==[0-9]+\.[0-9]+\.[0-9]+")

    def test_the_command_creates_the_output_and_refuses_unknown_machines(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "native-dist"
            argv = ["manylinux", "--out", str(output), "--engine", "podman"]
            with (
                patch("sys.argv", argv),
                patch.object(manylinux.platform, "machine", return_value="x86_64"),
                patch.object(manylinux.subprocess, "run") as run,
            ):
                manylinux.main()
            self.assertTrue(output.is_dir())
            run.assert_called_once_with(
                manylinux.command("podman", "x86_64", output.resolve(), None),
                check=True,
            )
            with (
                patch("sys.argv", argv),
                patch.object(manylinux.platform, "machine", return_value="riscv64"),
                patch.object(manylinux.subprocess, "run") as run,
                contextlib.redirect_stderr(io.StringIO()),
                self.assertRaises(SystemExit),
            ):
                manylinux.main()
            run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
