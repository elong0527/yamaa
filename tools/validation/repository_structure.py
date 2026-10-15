"""Repository file layout; native suites own language admission."""

from pathlib import Path

from tools.validation.editorial import example_spec_paths


def validate_examples_layout(root: Path):
    errors = []
    examples_dir = root / "benchmarks"
    if not examples_dir.exists():
        return errors

    for ex_dir in sorted(examples_dir.iterdir()):
        if not ex_dir.is_dir() or ex_dir.name.startswith("."):
            continue

        rel = ex_dir.relative_to(root)

        # Specification files
        spec_paths = example_spec_paths(ex_dir)
        if not spec_paths:
            errors.append(f"ERROR: {rel} missing spec.yaml or spec_<variant>.yaml")
        elif len(spec_paths) > 1 and any(
            path.name == "spec.yaml" for path in spec_paths
        ):
            errors.append(f"ERROR: {rel} cannot mix spec.yaml with variant specs")

        # input/
        if not (ex_dir / "input").is_dir():
            errors.append(f"ERROR: {rel} missing input/ directory")

        # expected/
        expected_dir = ex_dir / "expected"
        if not expected_dir.is_dir():
            errors.append(f"ERROR: {rel} missing expected/ directory")
        else:
            is_negative = ex_dir.name.startswith("negative-")
            has_error_yaml = (expected_dir / "error.yaml").exists()
            if is_negative and not has_error_yaml:
                errors.append(
                    f"ERROR: {rel} is negative but missing expected/error.yaml"
                )
            elif not is_negative and has_error_yaml:
                errors.append(f"ERROR: {rel} is positive but has expected/error.yaml")

            has_artifacts = any(
                f.is_file() for f in expected_dir.iterdir() if f.name != "error.yaml"
            )
            if not is_negative and not has_artifacts:
                errors.append(f"ERROR: {rel}/expected has no artifacts")

    return errors
