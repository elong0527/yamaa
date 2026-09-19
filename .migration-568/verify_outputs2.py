#!/usr/bin/env python3
"""Verify migrated specs produce byte-identical outputs.

Compares: OLD spec + OLD code (wt-main) vs NEW spec + NEW code (wt-lookup).
Both outputs are written to CSV, so type serialization is identical.
"""
import subprocess
import sys
from pathlib import Path

WT_MAIN = Path("/home/hatch/workspace/wt-main")
WT_LOOKUP = Path("/home/hatch/workspace/wt-lookup")
VENV = Path("/home/hatch/workspace/.yamaa-venv/bin/python")

result = subprocess.run(
    ["git", "diff", "--name-only", "HEAD", "--", "benchmark/"],
    cwd=WT_LOOKUP, capture_output=True, text=True
)
specs = sorted({
    p.split("/")[1] for p in result.stdout.splitlines() if p.endswith("/spec.yaml")
})
specs = [s for s in specs if not s.startswith("negative-")]

print(f"Checking {len(specs)} migrated specs...", file=sys.stderr)

def run_spec(wt, spec, pythonpath):
    """Run spec with the given worktree's code, return CSV bytes."""
    spec_dir = wt / "benchmark" / spec
    # Get the spec file from git HEAD for wt-main (old), working tree for wt-lookup (new)
    if wt == WT_MAIN:
        # Use the old spec from git, written into the spec dir for relative paths
        old_spec = subprocess.run(
            ["git", "show", f"HEAD:benchmark/{spec}/spec.yaml"],
            cwd=WT_LOOKUP, capture_output=True, text=True
        ).stdout
        tmp_spec = spec_dir / "_old_spec_tmp.yaml"
        tmp_spec.write_text(old_spec)
        spec_path = "_old_spec_tmp.yaml"
    else:
        spec_path = "spec.yaml"
        tmp_spec = None
    
    code = f"""
import yamaa, os
os.chdir("{spec_dir}")
pilot = yamaa.yamaa_domain("{spec_path}")
out = pilot.output
if out is not None:
    out.write_csv("/tmp/out-{wt.name}-{spec}.csv")
    print("OK", out.height)
else:
    print("NO OUTPUT")
    errs = pilot.issues.filter(pilot.issues["severity"] == "error")
    print(errs.height, "errors")
"""
    env = {"PYTHONPATH": str(pythonpath), "PATH": "/usr/bin:/bin"}
    result = subprocess.run(
        [str(VENV), "-c", code],
        cwd=str(spec_dir), capture_output=True, text=True, env=env
    )
    if tmp_spec and tmp_spec.exists():
        tmp_spec.unlink()
    return result

failed = []
for spec in specs:
    old = run_spec(WT_MAIN, spec, WT_MAIN / "python" / "src")
    new = run_spec(WT_LOOKUP, spec, WT_LOOKUP / "python" / "src")
    old_csv = Path(f"/tmp/out-wt-main-{spec}.csv")
    new_csv = Path(f"/tmp/out-wt-lookup-{spec}.csv")
    if not old_csv.exists() or not new_csv.exists():
        failed.append((spec, f"missing output: old={old_csv.exists()} new={new_csv.exists()}"))
        print(f"FAIL {spec}: missing output", file=sys.stderr)
        continue
    if old_csv.read_bytes() != new_csv.read_bytes():
        failed.append((spec, "output differs"))
        print(f"FAIL {spec}: output differs", file=sys.stderr)
    else:
        print(f"  OK {spec}", file=sys.stderr)

print(f"\n{len(specs) - len(failed)}/{len(specs)} byte-identical", file=sys.stderr)
for spec, reason in failed:
    print(f"FAIL {spec}: {reason}")
