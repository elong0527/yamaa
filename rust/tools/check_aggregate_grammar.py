"""Check independent aggregate truth through the retained typed Rust fixture."""

import random
from pathlib import Path

from check_numeric_grammar import contract_cases as numeric_contract_cases
from check_numeric_grammar import differential_sources as numeric_sources
from check_numeric_grammar import syntax_shape as numeric_shape

import yaml

WORKSPACE = Path(__file__).resolve().parents[1]


def shape(node):
    """Render the committed grammar's reducer and imported arithmetic notation."""
    kind = node["kind"]
    if kind == "reduction":
        return f"(reduce {node['name']} {shape(node['argument'])})"
    if kind == "star":
        return f"(star {node['dataset']})"
    if kind == "unary":
        return f"({'pos' if node['operator'] == '+' else 'neg'} {shape(node['value'])})"
    if kind == "binary":
        return f"({node['operator']} {shape(node['left'])} {shape(node['right'])})"
    if kind == "call":
        return (
            f"(call {node['name']} {' '.join(shape(arg) for arg in node['arguments'])})"
        )
    return numeric_shape(node)


def cases():
    """Load independent contract cases and expand each declared reducer form."""
    grammar = WORKSPACE.parent / "yaml/grammar"
    aggregate = yaml.safe_load((grammar / "aggregate.yaml").read_text(encoding="utf-8"))
    numeric = yaml.safe_load((grammar / "numeric.yaml").read_text(encoding="utf-8"))
    result = list(aggregate["cases"])
    for case in numeric_contract_cases(numeric)[len(numeric["cases"]) :]:
        # The imported grammar's ordinary numeric expressions remain valid here.
        result.append(
            {
                **case,
                "condition": case.get("condition", "").replace("numeric", "aggregate"),
            }
        )
    for name, declaration in aggregate["vocabulary"]["reducer"].items():
        for argument in ["A", "A+1", "ABS(A)", "D.*", "", "A,B", "SUM(A)"]:
            accepted = argument in {"A", "A+1", "ABS(A)"} or (
                argument == "D.*" and "star" in declaration["argument"]
            )
            result.append(
                {
                    "id": f"reducer-{name}-{argument}",
                    "text": f"{name.lower()}({argument})",
                    "parse": "accept" if accepted else "reject",
                    "condition": "nested_reduction"
                    if argument == "SUM(A)"
                    else "invalid_aggregate_expression",
                }
            )
    return result


def samples():
    """Exercise nested failures and written contexts with a reproducible seed."""
    rng = random.Random(1707)
    result = numeric_sources()
    atoms = [
        "SUM(A)",
        "COUNT(D.*)",
        "MIN(D.X)",
        "ONLY(NULL)",
        "SUM(ABS(B))",
        "A",
        "X",
        "1e9999",
    ]
    for _ in range(700):
        left, right = rng.choice(atoms), rng.choice(atoms)
        result.append(
            rng.choice(
                [
                    f"{left} + {right}",
                    f"SUM({left} + {right})",
                    f"COALESCE({left},{right})",
                    f"{left} > {right}",
                    f"COUNT({left}",
                    f"\u2003-{left} / {right}",
                ]
            )
        )
    result.extend(
        [
            "SUM(COALESCE(MIN(A), MAX(B)))",
            "SUM(MAX(MIN(A)))",
            "COUNT((D.*))",
            "COUNT(TRUE.*)",
            "COUNT(D .*)",
            "SUM(" + "9" * 5000 + ")",
        ]
    )
    return result


def main():
    """Check authored truth and preserve the old input inventory as robustness."""
    from native_syntax_checks import assert_contract, exact_controls, replay

    contract = cases()
    sources = [case["text"] for case in contract] + samples()
    responses = replay("aggregate", sources)
    for case, response in zip(contract, responses, strict=False):
        assert_contract(case, response["outcome"], shape)
    controls = exact_controls("aggregate")
    print(
        f"Aggregate typed syntax: {len(contract)} independent contract checks, "
        f"{controls} complete authored controls; {len(sources)} robustness observations "
        "(historical reference comparisons are not rerun or claimed as semantic truth)"
    )


if __name__ == "__main__":
    main()
