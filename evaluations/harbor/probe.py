"""Exercise the submission worker's filesystem and process boundary in Docker."""

from __future__ import annotations

import json
import time
from pathlib import Path

import grade
import sandbox


def main() -> None:
    app, tests, logs = Path("/app"), Path("/tests"), Path("/logs/verifier")
    sandbox.prepare(app, tests, logs)
    reference = tests / "reference/result.py"
    original = reference.read_text()
    prefix = """from pathlib import Path
import subprocess
def denied(operation):
    try:
        operation()
    except PermissionError:
        return
    raise AssertionError("worker accessed a protected path")
denied(lambda: Path('/tests/expected/adsl.csv').read_text())
denied(lambda: Path('/tests/reference/result.py').write_text('tampered'))
denied(lambda: Path('/logs/verifier/reward.json').write_text('tampered'))
denied(lambda: Path('/app/input/dm.csv').write_text('tampered'))
denied(lambda: Path('/app/output/result.py').unlink())
subprocess.Popen(['sh', '-c', 'sleep 1; touch /tmp/yamaa-escaped-worker'],
                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                 start_new_session=True)
"""
    script = app / "output/result.py"
    script.write_text(prefix)
    code, log = sandbox.run(["python3", str(script)], app, 60)
    if code != 0:
        raise RuntimeError(log)
    time.sleep(1.2)
    if Path("/tmp/yamaa-escaped-worker").exists():
        raise RuntimeError("a worker descendant survived cleanup")
    script.write_text(original)
    code, log = sandbox.run(["python3", str(script)], app, 60)
    if code != 0:
        raise RuntimeError(log)
    contract = json.loads((tests / "contract.json").read_text())
    result = grade.grade(
        contract,
        tests / "expected",
        app / "output",
        Path("/absent.json"),
        rerun=True,
        run=sandbox.run,
        reference=reference,
    )
    if not result["passed"]:
        raise RuntimeError(f"a valid script failed the sandbox probes: {result}")
    script.write_text(
        "from pathlib import Path\nPath('/app/output/adsl.csv').symlink_to('/tests/expected/adsl.csv')\n"
    )
    code, log = sandbox.run(["python3", str(script)], app, 60)
    if code != 0:
        raise RuntimeError(log)
    if grade.grade_output(contract["outputs"][0], tests / "expected", app / "output")[
        "passed"
    ]:
        raise RuntimeError("a golden-file symlink passed grading")
    print("filesystem, process cleanup and symlink probes passed")


if __name__ == "__main__":
    main()
