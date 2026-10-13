"""Check independent numeric truth through the retained typed Rust fixture."""

from pathlib import Path

from check_numeric_grammar import contract_cases as numeric_contract_cases
from check_numeric_grammar import differential_sources as numeric_sources
from check_numeric_grammar import syntax_shape as numeric_shape

import yaml

WORKSPACE = Path(__file__).resolve().parents[1]


def cases():
    """Reuse every independent committed numeric case and vocabulary expansion."""
    contract = yaml.safe_load(
        (WORKSPACE.parent / "yaml/grammar/numeric.yaml").read_text(encoding="utf-8")
    )
    return numeric_contract_cases(contract)


def main():
    """Check authored truth; retain all old differential inputs as robustness only."""
    from native_syntax_checks import assert_contract, exact_controls, replay

    contract = cases()
    sources = [case["text"] for case in contract] + numeric_sources()
    responses = replay("numeric", sources)
    for case, response in zip(contract, responses, strict=False):
        assert_contract(case, response["outcome"], numeric_shape)
    controls = exact_controls("numeric")
    print(
        f"Numeric typed syntax: {len(contract)} independent contract checks, "
        f"{controls} complete authored controls; {len(sources)} robustness observations "
        "(historical reference comparisons are not rerun or claimed as semantic truth)"
    )


if __name__ == "__main__":
    main()
