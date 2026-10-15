"""Create the MSVC import library needed to test the R binding on Windows."""

import os
import re
import subprocess
from pathlib import Path


def export_definition(dumpbin_output):
    """Project named DLL exports, including aliases, into a module definition."""
    exports = re.findall(
        r"^\s+\d+\s+[0-9A-Fa-f]+\s+[0-9A-Fa-f]+\s+(\S+)",
        dumpbin_output,
        re.MULTILINE,
    )
    if not exports or len(exports) != len(set(exports)):
        raise ValueError("R.dll export table is empty or contains duplicate names")
    return "LIBRARY R.dll\nEXPORTS\n" + "".join(
        f"    {name}\n" for name in sorted(exports)
    )


def main():
    if os.name != "nt":
        raise SystemExit("R import-library generation requires Windows MSVC tools")
    installer = (
        Path(os.environ["ProgramFiles(x86)"]) / "Microsoft Visual Studio/Installer"
    )
    installation = Path(
        subprocess.check_output(
            [
                str(installer / "vswhere.exe"),
                "-latest",
                "-products",
                "*",
                "-requires",
                "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
                "-property",
                "installationPath",
            ],
            text=True,
        ).strip()
    )
    version = (
        (installation / "VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt")
        .read_text()
        .strip()
    )
    tools = installation / "VC/Tools/MSVC" / version / "bin/Hostx64/x64"
    r_bin = Path(
        subprocess.check_output(
            ["Rscript", "--vanilla", "-e", "cat(R.home('bin'))"], text=True
        ).strip()
    )
    exports = subprocess.check_output(
        [str(tools / "dumpbin.exe"), "/NOLOGO", "/EXPORTS", str(r_bin / "R.dll")],
        text=True,
    )
    definition = r_bin / "R.def"
    definition.write_text(export_definition(exports), encoding="ascii")
    subprocess.run(
        [
            str(tools / "lib.exe"),
            "/NOLOGO",
            f"/DEF:{definition}",
            f"/OUT:{r_bin / 'R.lib'}",
            "/MACHINE:X64",
        ],
        check=True,
    )
    print(r_bin / "R.lib")


if __name__ == "__main__":
    main()
