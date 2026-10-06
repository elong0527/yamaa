"""Supplement independent R022 truth with deterministic Node Unicode-mode comparisons.

Node is a development/test oracle only. It is not a runtime dependency or a
replacement for repository restrictions, authored expectations or full qualification.
"""

import argparse
import itertools
import json
import subprocess
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parents[1]


def requests():
    """Exercise supported patterns, captures and short subjects in a stable order."""
    atoms = [
        "a",
        "b",
        ".",
        r"\d",
        r"\D",
        r"\w",
        r"\W",
        r"\s",
        r"\S",
        r"\b",
        r"\B",
        "[ab]",
        "[^a]",
        "[a-c]",
        "[]",
        "[^]",
        r"[\S]",
        r"[^\S]",
        r"\u{1d400}",
        r"\x61",
        r"\cA",
    ]
    patterns = set(atoms)
    for a in atoms:
        patterns.update(
            [f"^{a}$", f"(?:{a})*", f"({a})?", f"(?:{a}){{2}}", f"{a}|b", f"a{a}b"]
        )
    for a in ["a", "b", "[ab]", ".", r"\d", "(?:a|b)", "(?:a?)"]:
        for q in ["?", "*", "+", "??", "*?", "+?", "{0}", "{1,2}", "{1,2}?"]:
            patterns.update(
                [
                    f"({a}{q})a",
                    f"({a}){q}",
                    f"(({a}){q})b",
                    f"({a}{q})\\1",
                    f"({a}){q}\\1",
                ]
            )
    patterns.update(
        [
            r"(a|(b))+",
            r"(a|(b))*?",
            r"(a(b)?)+",
            r"(?=(a+))a*b\1",
            r"(?<=(a)(b))c",
            r"(?<=(a|b){2})c",
            r"(?<=((a)|(b)){2})c",
            r"(?<!a)b",
            r"(?<=a|b)c",
            r"(?=(a))\1",
            r"(?!(a))\1b",
            r"\1(a)",
            r"(?<x>a)\k<x>",
            r"\k<x>(?<x>a)",
            r"(a\1)",
            r"((a)?b)*",
            r"((a*)*)*",
            r"(a?){2,4}",
            r"(a?){2,4}?",
            r"(?:(?=a)|b)*",
            r"(a)(?<=\1)b",
            r"(a){1,3}(?<=\1)b",
            r"(?<=\1(a))b",
            r"(?<=(a)\1)b",
            r"(?<=\1\2(?:(a)bb|(aa)))c",
            r"(?<=(?:\1(a)){2})b",
            r"(?<=(?:(a)\1){2})b",
            r"(?:(a)|(b)){2}(?<=\1\2)c",
            r"(?<x>a)(?<=\k<x>)b",
            r"(?<=\1)(a*)",
            r"()a(?<=\1+)b",
            r"(?=(a))a(?<=\1)b",
            r"(?!(a+))(?<=\1)b",
            r"(a)(?<!(?<=\1)b)c",
            r"(){0,3}(?<=\1)b",
            r"(){2,3}(?<=\1)b",
            r"(a){0}(?<=\1)b",
            r"(\1a)(?<=\1)b",
            r"(\2a)(\1b)(?<=\1\2)c",
        ]
    )
    subjects = [
        "".join(x) for n in range(4) for x in itertools.product("ab", repeat=n)
    ] + [
        "abc",
        "aabc",
        "aba",
        "aab",
        "bbb",
        "ac",
        "bc",
        "1",
        "a1b",
        " ",
        "\n",
        "a\n",
        "\ufeff",
        "\u0085",
        "\u0665",
        "\u00e9",
        "\U0001d400",
        "\x01",
        "aaab",
        "baaabac",
        "aabbc",
        "aaaac",
        "aaaab",
    ]
    return [{"pattern": p, "subject": s} for p in sorted(patterns) for s in subjects]


def main():
    """Report all mismatches; never update independent expectations from either engine."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--node", default="node")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    observations = requests()
    run = subprocess.run(
        [
            "cargo",
            "run",
            "--offline",
            "--quiet",
            "-p",
            "yamaa-adapters",
            "--example",
            "regex_probe",
        ],
        cwd=WORKSPACE,
        input="".join(json.dumps(request) + "\n" for request in observations),
        text=True,
        encoding="utf-8",
        capture_output=True,
        check=True,
    )
    actual = [json.loads(line) for line in run.stdout.split("\n") if line]
    js = r"""const fs=require('fs');let rows=JSON.parse(fs.readFileSync(0,'utf8')); console.log(JSON.stringify(rows.map(({pattern:p,subject:s})=>{try{let re=new RegExp(p,'u');let m=re.exec(s);let count=new RegExp('(?:'+p+')|','u').exec('').length-1;return {accepted:true,group_count:count,full_match:new RegExp('^(?:'+p+')(?![\\s\\S])','u').test(s),match:m&&Array.from(m,x=>x===undefined?null:x)};}catch(e){return {accepted:false}}})));"""

    expected = json.loads(
        subprocess.check_output(
            [args.node, "-e", js],
            input=json.dumps(observations),
            text=True,
            encoding="utf-8",
        )
    )
    assert len(actual) == len(expected) == len(observations)
    mismatches = [
        {"request": request, "rust": got, "node": reference}
        for request, got, reference in zip(observations, actual, expected, strict=True)
        if got != reference
    ]
    report = {
        "qualification": "not-qualified",
        "node_version": subprocess.check_output(
            [args.node, "--version"], text=True
        ).strip(),
        "patterns": len({request["pattern"] for request in observations}),
        "observations": len(observations),
        "mismatches": mismatches,
    }
    rendered = json.dumps(report, ensure_ascii=True, indent=2) + "\n"
    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
    else:
        print(rendered, end="")
    raise SystemExit(bool(mismatches))


if __name__ == "__main__":
    main()
