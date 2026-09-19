#!/usr/bin/env python3
"""Migrate benchmark specs to the unified explicit lookup (#568).

Uses ruamel.yaml to preserve formatting, comments, and style.

Transforms:
1. `record_lookups:` -> `lookups:` (unmatched/incomplete -> missing/strict)
2. Implicit cross-dataset scalar `source: DS.COL` -> explicit `lookup:` block
3. Cross-dataset `aggregate:` -> add explicit `source:`/`key:` pairs

Uses .migration-568/join_keys.json for the inferred key pairs.
"""
import json
import sys
from pathlib import Path

from ruamel.yaml import YAML

REPO = Path("/home/hatch/workspace/wt-lookup")
DUMP = json.load(open(REPO / ".migration-568/join_keys.json"))

yaml = YAML()
yaml.preserve_quotes = True
yaml.width = 4096
yaml.indent(mapping=2, sequence=4, offset=2)


def navigate(doc, spec_path):
    """Navigate to the parent of the target node via spec_path."""
    parts = spec_path.split(".")
    node = doc
    for part in parts[:-1]:
        if isinstance(node, dict) and part in node:
            node = node[part]
        elif isinstance(node, list):
            node = next(
                (c for c in node if isinstance(c, dict) and c.get("name") == part),
                None,
            )
        else:
            return None, None
        if node is None:
            return None, None
    return node, parts[-1]


def migrate_record_lookups(doc, spec_dir):
    """record_lookups -> lookups with unified absence vocabulary.
    
    If source/key are omitted, the old R015-5 used the applicable output
    keys. The migration makes these explicit.
    """
    if "record_lookups" not in doc:
        return False
    lookups = doc.pop("record_lookups")
    output_keys = doc.get("keys", [])
    for lookup in lookups:
        unmatched = lookup.pop("unmatched", None)
        incomplete = lookup.pop("incomplete", None)
        if unmatched == "fail" or incomplete == "fail":
            lookup["strict"] = True
        # unmatched/incomplete: missing -> default (missing), dropped
        # R015-5: no source/key means use applicable output keys. Make explicit.
        if "source" not in lookup or "key" not in lookup:
            dataset = lookup.get("dataset")
            if dataset and output_keys:
                # Read dataset header to find which output keys it has
                input_decl = doc.get("input", {}).get(dataset, {})
                csv_path = input_decl.get("path") if isinstance(input_decl, dict) else input_decl
                if csv_path:
                    csv_file = spec_dir / csv_path
                    if csv_file.exists():
                        with open(csv_file) as f:
                            header = f.readline().strip().split(",")
                        applicable = [k for k in output_keys if k in header]
                        if applicable:
                            lookup["source"] = list(applicable)
                            lookup["key"] = list(applicable)
                            print(f"    Added explicit keys {applicable} to lookup {lookup['id']}")
    # Insert lookups where record_lookups was (after input, before output/base)
    items = list(doc.items())
    doc.clear()
    inserted = False
    for key, value in items:
        if not inserted and key in ("base", "output"):
            doc["lookups"] = lookups
            inserted = True
        doc[key] = value
    if not inserted:
        doc["lookups"] = lookups
    return True


def migrate_scalar_join(doc, join):
    """Convert implicit cross-dataset scalar source to explicit lookup."""
    parent, leaf = navigate(doc, join["spec_path"])
    if parent is None or leaf != "source":
        print(f"  SKIP {join['spec_path']}: cannot navigate", file=sys.stderr)
        return False
    from ruamel.yaml.comments import CommentedSeq

    def flow(seq):
        s = CommentedSeq(seq)
        s.fa.set_flow_style()
        return s

    source = parent.get("source")
    keys = join["keys"]
    lookup = yaml.map()
    if isinstance(source, str) and "." in source:
        dataset, column = source.split(".", 1)
        if dataset != join["dataset"]:
            print(f"  SKIP {join['spec_path']}: dataset mismatch", file=sys.stderr)
            return False
        lookup["dataset"] = dataset
        lookup["source"] = flow(keys)
        lookup["key"] = flow(keys)
        lookup["value"] = column
    elif isinstance(source, dict):
        # Filtered source with multiple_matches -> inline lookup.
        variable = source.get("variable")
        if not isinstance(variable, str) or "." not in variable:
            print(f"  SKIP {join['spec_path']}: bad variable", file=sys.stderr)
            return False
        dataset, column = variable.split(".", 1)
        lookup["dataset"] = dataset
        lookup["source"] = flow(keys)
        lookup["key"] = flow(keys)
        lookup["value"] = column
        if "filter" in source:
            lookup["filter"] = source["filter"]
        multiple = source.get("multiple_matches") or {}
        if "order_by" in multiple:
            terms = []
            for term in multiple["order_by"]:
                t = yaml.map()
                t["variable"] = term
                terms.append(t)
            lookup["order_by"] = terms
        if "keep" in multiple:
            lookup["keep"] = multiple["keep"]
    else:
        print(f"  SKIP {join['spec_path']}: not a qualified source", file=sys.stderr)
        return False
    # Move sibling keys into the lookup block
    for key in list(parent.keys()):
        if key != "source":
            lookup[key] = parent.pop(key)
    parent.clear()
    parent["lookup"] = lookup
    return True


def migrate_aggregate_join(doc, join):
    """Add explicit source/key pairs to a cross-dataset aggregate."""
    parent, leaf = navigate(doc, join["spec_path"])
    if parent is None:
        print(f"  SKIP {join['spec_path']}: cannot navigate", file=sys.stderr)
        return False
    if leaf == "expr":
        agg = parent
    elif leaf == "aggregate":
        agg = parent.get("aggregate")
        if isinstance(agg, str):
            # Bare-string aggregate: expand to a mapping to hold the pairs.
            agg = yaml.map()
            agg["expr"] = parent.pop("aggregate")
            # Preserve key order: expr first, then source/key
            parent["aggregate"] = agg
    else:
        print(f"  SKIP {join['spec_path']}: unexpected leaf {leaf}", file=sys.stderr)
        return False
    if not isinstance(agg, dict):
        print(f"  SKIP {join['spec_path']}: aggregate not a mapping", file=sys.stderr)
        return False
    if "source" in agg or "key" in agg:
        print(f"  SKIP {join['spec_path']}: already has source/key", file=sys.stderr)
        return False
    keys = join["keys"]
    from ruamel.yaml.comments import CommentedSeq
    source_seq = CommentedSeq(keys)
    source_seq.fa.set_flow_style()
    key_seq = CommentedSeq(keys)
    key_seq.fa.set_flow_style()
    agg["source"] = source_seq
    agg["key"] = key_seq
    return True


def migrate_spec(spec_name):
    path = REPO / "benchmark" / spec_name / "spec.yaml"
    spec_dir = path.parent
    with open(path) as f:
        doc = yaml.load(f)
    changed = False
    data = DUMP.get(spec_name, {})

    if migrate_record_lookups(doc, spec_dir):
        print(f"  record_lookups -> lookups")
        changed = True

    for join in data.get("joins", []):
        spec_path = join["spec_path"]
        if spec_path.endswith(".derivation.source"):
            if migrate_scalar_join(doc, join):
                print(f"  scalar join -> lookup: {spec_path}")
                changed = True
        elif ".derivation.aggregate" in spec_path:
            if migrate_aggregate_join(doc, join):
                print(f"  aggregate + source/key: {spec_path}")
                changed = True

    if changed:
        with open(path, "w") as f:
            yaml.dump(doc, f)
    return changed


def main():
    specs = sys.argv[1:] or sorted(DUMP.keys())
    for spec in specs:
        print(f"Migrating {spec}...")
        try:
            if migrate_spec(spec):
                print(f"  WROTE {spec}")
        except Exception as e:
            print(f"  ERROR {spec}: {e}", file=sys.stderr)
            import traceback
            traceback.print_exc()


if __name__ == "__main__":
    main()
