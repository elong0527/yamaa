"""Bounded reviews must cover exact Git leaves, complete context and live heads."""

import copy
import unittest
from unittest.mock import Mock

from tools.ci import review_coverage as coverage
from tools.ci import review_gate as gate

REPOSITORY = "elong0527/yamaa"
REPOSITORY_ID = 1299533902
NUMBER, BATCHES = 42, (101, 102)
REPO = {"full_name": REPOSITORY, "id": REPOSITORY_ID}


def oid(number):
    # Synthetic Git object IDs, not content digests produced by the verifier.
    return f"{number:040x}"


MAIN, ORIGINAL, HEAD = oid(10), oid(11), oid(12)
CHUNK_HEADS, CHUNK_BASES = (oid(20), oid(21)), (oid(30), oid(31))
TARGET_TREE = oid(2002)


def review(
    head, *, identity=7, state="success", description="Review completed", **fields
):
    return {
        "id": identity,
        "state": state,
        "description": description,
        "context": "CodeRabbit",
        "creator": dict(gate.REVIEW_CREATOR),
        "url": gate.status_url(REPOSITORY, head),
        "created_at": "2026-10-15T14:16:05Z",
        **fields,
    }


def pull(number, head, base, *, branch, count, body=None):
    return {
        "number": number,
        "state": "open",
        "merged": False,
        "changed_files": count,
        "body": body,
        "head": {"sha": head, "ref": f"review-head-{number}", "repo": dict(REPO)},
        "base": {"sha": base, "ref": branch, "repo": dict(REPO)},
    }


def tree(identity, leaves):
    return {
        "sha": identity,
        "truncated": False,
        "tree": [
            {"path": name, "mode": mode, "type": kind, "sha": value}
            for name, (mode, kind, value) in sorted(leaves.items())
        ],
    }


class Payloads:
    """Independent API fixtures: 301 file changes and 302 raw leaf paths.

    The changed leaves include an executable-mode-only change, a symlink,
    a gitlink, a rename, an addition and a deletion. An unchanged context
    file and a main-only change distinguish full context and merge-base use.
    """

    repository, repository_id = REPOSITORY, REPOSITORY_ID

    def __init__(self, groups=None):
        groups = groups or (range(150), range(150, 301))
        self.old, self.new, self.files = {}, {}, {}
        for index in range(301):
            name = f"source/f{index:03}.rs"
            old = ("100644", "blob", oid(10000 + index))
            new = ("100644", "blob", oid(20000 + index))
            if index == 0:
                new = ("100755", "blob", old[2])
            elif index == 1:
                new = ("120000", "blob", new[2])
            elif index == 2:
                old = ("160000", "commit", old[2])
                new = ("160000", "commit", new[2])
            if index != 4:
                self.old[name] = old
            destination = "source/renamed.rs" if index == 3 else name
            if index != 5:
                self.new[destination] = new
            status = {3: "renamed", 4: "added", 5: "removed"}.get(index, "modified")
            row = {
                "filename": destination,
                "status": status,
                "sha": old[2] if index == 5 else new[2],
            }
            if index == 3:
                row["previous_filename"] = name
            self.files[index] = row
        context = ("100644", "blob", oid(9999))
        self.old["unchanged/context.py"] = context
        self.new["unchanged/context.py"] = context
        self.requests, self.history, self.published = {}, [], []
        self.pull_versions = {
            NUMBER: [
                pull(
                    NUMBER,
                    HEAD,
                    MAIN,
                    branch="main",
                    count=301,
                    body="The implementation.\n<!-- yamaa-review-batches: [101, 102] -->",
                )
            ]
        }
        self.file_pages = {NUMBER: list(self.files.values())}
        self.status_versions = {
            HEAD: [
                [
                    review(
                        HEAD,
                        description="Review skipped: 301 files exceed the limit of 300",
                    )
                ]
            ]
        }
        self.thread_versions = {}
        self.timeline_versions, self.comment_versions = {}, {}
        self.add_commit(ORIGINAL, oid(2000), self.old)
        self.add_commit(
            MAIN,
            oid(2001),
            {**self.old, "main-only.rs": ("100644", "blob", oid(9998))},
        )
        self.add_commit(HEAD, TARGET_TREE, self.new)
        self.add_compare(MAIN, HEAD, ORIGINAL)
        for number, head, base, group, identity in zip(
            BATCHES,
            CHUNK_HEADS,
            CHUNK_BASES,
            groups,
            (oid(2003), oid(2004)),
            strict=True,
        ):
            group = list(group)
            restored = dict(self.new)
            for index in group:
                name = f"source/f{index:03}.rs"
                destination = "source/renamed.rs" if index == 3 else name
                restored.pop(destination, None)
                if index != 4:
                    restored[name] = self.old[name]
            self.add_commit(base, identity, restored, parents=(HEAD,))
            self.add_commit(head, TARGET_TREE, self.new, parents=(base,))
            self.add_compare(base, head, base)
            self.pull_versions[number] = [
                pull(
                    number, head, base, branch=f"review-base-{number}", count=len(group)
                )
            ]
            self.file_pages[number] = [self.files[index] for index in group]
            self.status_versions[head] = [[review(head)]]
            self.timeline_versions[number] = [[{"event": "committed", "sha": head}]]
            self.comment_versions[number] = [
                [
                    {
                        "id": number + 1000,
                        "user": dict(gate.REVIEW_CREATOR),
                        "issue_url": f"https://api.github.com/repos/{REPOSITORY}/issues/{number}",
                        "created_at": "2026-10-15T14:00:26Z",
                        "updated_at": "2026-10-15T14:16:02Z",
                        "body": (
                            "Reviewing files that changed from the base of the PR and between "
                            f"{base} and {head}.\n"
                            '<!-- final_review_risk_coverage:{"sourceCommitId":"'
                            + head
                            + '","coveredCommitId":"'
                            + head
                            + '","kind":"reviewed"} -->'
                        ),
                    }
                ]
            ]

    @staticmethod
    def next(versions):
        return copy.deepcopy(versions.pop(0) if len(versions) > 1 else versions[0])

    def add_commit(self, commit, identity, leaves, *, parents=()):
        self.requests[f"git/commits/{commit}"] = {
            "sha": commit,
            "tree": {"sha": identity},
            "parents": [{"sha": parent} for parent in parents],
        }
        self.requests[f"git/trees/{identity}?recursive=1"] = tree(identity, leaves)

    def add_compare(self, base, head, merge_base):
        self.requests[f"compare/{base}...{head}?per_page=1"] = {
            "base_commit": {"sha": base},
            "merge_base_commit": {"sha": merge_base},
            # Deliberately incomplete: a verifier must not trust compare.files.
            "files": [],
        }

    def request(self, method, endpoint, body=None):
        self.history.append(("request", method, endpoint))
        assert method == "GET" and body is None
        return copy.deepcopy(
            self.requests[endpoint.removeprefix(f"repos/{REPOSITORY}/")]
        )

    def pull(self, number):
        self.history.append(("pull", number))
        return self.next(self.pull_versions[number])

    def pages(self, endpoint):
        self.history.append(("pages", endpoint))
        if endpoint.startswith("pulls/") and endpoint.endswith("/files"):
            return copy.deepcopy(self.file_pages[int(endpoint.split("/")[1])])
        if endpoint.startswith("issues/") and endpoint.endswith("/timeline"):
            return self.next(self.timeline_versions[int(endpoint.split("/")[1])])
        if endpoint.startswith("issues/") and endpoint.endswith("/comments"):
            return self.next(self.comment_versions[int(endpoint.split("/")[1])])
        if endpoint == "pulls?state=open&base=main":
            return [copy.deepcopy(self.pull_versions[NUMBER][0])]
        if endpoint.startswith("commits/") and endpoint.endswith("/pulls"):
            head = endpoint.split("/")[1]
            return [
                copy.deepcopy(versions[0])
                for versions in self.pull_versions.values()
                if versions[0]["head"]["sha"] == head
            ]
        raise AssertionError(endpoint)

    def statuses(self, head):
        self.history.append(("statuses", head))
        return self.next(self.status_versions[head])

    def threads(self, number, head):
        self.history.append(("threads", number, head))
        return self.next(self.thread_versions.setdefault(number, [[]]))

    def publish(self, verdict):
        self.history.append(("publish", verdict.head, verdict.state))
        self.published.append(verdict)

    def base_rows(self, index=0):
        return self.requests[f"git/trees/{oid(2003 + index)}?recursive=1"]["tree"]


class Metadata(unittest.TestCase):
    def test_optional_json_metadata_has_one_unambiguous_list(self):
        self.assertEqual(coverage.batch_numbers(None), ())
        self.assertEqual(coverage.batch_numbers("ordinary description"), ())
        self.assertEqual(
            coverage.batch_numbers(
                "before\n<!-- yamaa-review-batches: [17, 28] -->\nafter"
            ),
            (17, 28),
        )
        for body in [
            "yamaa-review-batches: [17]",
            "<!-- yamaa-review-batches: [17] --> <!-- yamaa-review-batches: [28] -->",
            *(
                f"<!-- yamaa-review-batches: {value} -->"
                for value in [
                    "[]",
                    "null",
                    "{}",
                    "[17,17]",
                    "[true]",
                    "[0]",
                    "[-1]",
                    '["17"]',
                    "[17,]",
                    "[17]; run code",
                ]
            ),
        ]:
            with self.subTest(body=body), self.assertRaises(coverage.CoverageError):
                coverage.batch_numbers(body)

    def test_only_explicit_large_file_limit_skips_qualify_for_fallback(self):
        self.assertTrue(
            coverage.large_skip(
                (7, "success", "Review skipped: 301 files exceed the limit of 300")
            )
        )
        for evidence in [
            None,
            (7, "pending", "Review skipped: 301 files exceed the limit of 300"),
            (7, "success", "Review skipped"),
            (7, "success", "Review skipped: 300 files exceed the limit of 300"),
            (7, "success", "Review skipped: 301 files exceed the limit of 500"),
            (7, "success", "Review completed with skipped files"),
        ]:
            with self.subTest(evidence=evidence):
                self.assertFalse(coverage.large_skip(evidence))


class ExactTrees(unittest.TestCase):
    def test_disjoint_exact_deltas_preserve_complete_context_and_all_leaf_types(self):
        api = Payloads()
        proof = coverage.prove(api, NUMBER, HEAD)
        self.assertEqual([chunk.number for chunk in proof.chunks], list(BATCHES))
        self.assertEqual(proof.target.head, HEAD)
        proof.recheck(api)
        self.assertEqual(
            [row[1] for row in api.history if row[0] == "pull"][-3:],
            [NUMBER, *BATCHES],
        )
        self.assertEqual(api.published, [])

    def test_300_raw_paths_pass_and_a_301_path_chunk_refuses_even_with_300_files(self):
        # The rename contributes two paths but is one PR file.
        coverage.prove(Payloads((range(299), range(299, 301))), NUMBER, HEAD)
        with self.assertRaisesRegex(coverage.CoverageError, "300 raw leaf paths"):
            coverage.prove(Payloads((range(300), range(300, 301))), NUMBER, HEAD)

    def test_gaps_and_overlaps_are_not_review_coverage(self):
        for groups, message in [
            ((range(150), range(151, 301)), "unreviewed"),
            ((range(150), range(149, 301)), "overlap"),
        ]:
            with (
                self.subTest(groups=groups),
                self.assertRaisesRegex(coverage.CoverageError, message),
            ):
                coverage.prove(Payloads(groups), NUMBER, HEAD)

    def test_empty_chunks_and_small_target_diffs_cannot_use_the_fallback(self):
        with self.assertRaisesRegex(coverage.CoverageError, "300 raw leaf paths"):
            coverage.prove(Payloads((range(0), range(301))), NUMBER, HEAD)
        api = Payloads()
        api.pull_versions[NUMBER][0]["changed_files"] = 300
        with self.assertRaisesRegex(coverage.CoverageError, "actual large diff"):
            coverage.prove(api, NUMBER, HEAD)

    def test_directory_entries_are_not_counted_as_changed_git_leaves(self):
        api = Payloads()
        api.base_rows().append(
            {"path": "source", "mode": "040000", "type": "tree", "sha": oid(999)}
        )
        coverage.prove(api, NUMBER, HEAD)

    def test_exact_old_blob_mode_symlink_and_gitlink_facts_must_match(self):
        for index, field, value in [
            (0, "mode", "120000"),
            (1, "mode", "120000"),
            (2, "sha", oid(98765)),
            (10, "sha", oid(98765)),
        ]:
            api = Payloads()
            row = next(
                row
                for row in api.base_rows()
                if row["path"] == f"source/f{index:03}.rs"
            )
            row[field] = value
            with (
                self.subTest(index=index, field=field),
                self.assertRaisesRegex(coverage.CoverageError, "old/new Git leaves"),
            ):
                coverage.prove(api, NUMBER, HEAD)

    def test_no_unrelated_base_change_can_hide_in_a_batch(self):
        api = Payloads()
        row = next(
            row for row in api.base_rows() if row["path"] == "unchanged/context.py"
        )
        row["sha"] = oid(98765)
        with self.assertRaisesRegex(coverage.CoverageError, "old/new Git leaves"):
            coverage.prove(api, NUMBER, HEAD)

    def test_every_chunk_has_the_exact_complete_target_tree(self):
        api = Payloads()
        modified = {**api.new, "unchanged/context.py": ("100644", "blob", oid(98765))}
        api.add_commit(CHUNK_HEADS[0], oid(999), modified)
        with self.assertRaisesRegex(
            coverage.CoverageError, "complete target source tree"
        ):
            coverage.prove(api, NUMBER, HEAD)

    def test_reused_head_status_cannot_prove_multiple_independent_batch_reviews(self):
        for replacement in [HEAD, CHUNK_HEADS[0]]:
            api = Payloads()
            api.pull_versions[BATCHES[1]][0]["head"]["sha"] = replacement
            with (
                self.subTest(replacement=replacement),
                self.assertRaisesRegex(
                    coverage.CoverageError, "distinct head commits|exclusively"
                ),
            ):
                coverage.prove(api, NUMBER, HEAD)

    def test_chunk_review_merge_base_must_be_the_declared_synthetic_base(self):
        api = Payloads()
        api.add_compare(CHUNK_BASES[0], CHUNK_HEADS[0], ORIGINAL)
        with self.assertRaisesRegex(coverage.CoverageError, "actual review merge base"):
            coverage.prove(api, NUMBER, HEAD)

    def test_both_tree_truncation_and_missing_truncation_flag_fail_closed(self):
        for truncated in [True, None, "false"]:
            api = Payloads()
            api.requests[f"git/trees/{TARGET_TREE}?recursive=1"]["truncated"] = (
                truncated
            )
            with (
                self.subTest(truncated=truncated),
                self.assertRaisesRegex(coverage.CoverageError, "truncated"),
            ):
                coverage.prove(api, NUMBER, HEAD)

    def test_invalid_commit_tree_path_mode_type_object_and_duplicate_paths_refuse(self):
        for field, value in [
            ("path", "../outside"),
            ("path", "/absolute"),
            ("mode", "100600"),
            ("type", "tree"),
            ("sha", "not-an-object"),
        ]:
            api = Payloads()
            api.base_rows()[0][field] = value
            with self.subTest(field=field), self.assertRaises(coverage.CoverageError):
                coverage.prove(api, NUMBER, HEAD)
        api = Payloads()
        api.base_rows().append(dict(api.base_rows()[0]))
        with self.assertRaisesRegex(coverage.CoverageError, "repeated Git tree"):
            coverage.prove(api, NUMBER, HEAD)
        api = Payloads()
        api.requests[f"git/commits/{HEAD}"]["sha"] = ORIGINAL
        with self.assertRaisesRegex(coverage.CoverageError, "commit identity"):
            coverage.prove(api, NUMBER, HEAD)

    def test_renames_and_file_pagination_must_match_the_complete_raw_delta(self):
        for mutation in ["omit", "duplicate", "rename", "object", "extra"]:
            api = Payloads()
            rows = api.file_pages[BATCHES[0]]
            if mutation == "omit":
                rows.pop()
            elif mutation == "duplicate":
                rows[-1] = copy.deepcopy(rows[0])
            elif mutation == "rename":
                rows[3]["previous_filename"] = "source/not-in-original.rs"
            elif mutation == "object":
                rows[0]["sha"] = oid(98765)
            else:
                rows[-1]["filename"] = "not-a-change.rs"
            with (
                self.subTest(mutation=mutation),
                self.assertRaises(coverage.CoverageError),
            ):
                coverage.prove(api, NUMBER, HEAD)

    def test_self_reference_wrong_repository_main_base_closed_or_merged_chunks_refuse(
        self,
    ):
        for mutation in ["self", "head-repo", "base-repo", "main", "closed", "merged"]:
            api = Payloads()
            chunk = api.pull_versions[BATCHES[0]][0]
            if mutation == "self":
                api.pull_versions[NUMBER][0]["body"] = (
                    "<!-- yamaa-review-batches: [42] -->"
                )
            elif mutation == "head-repo":
                chunk["head"]["repo"]["id"] = 1
            elif mutation == "base-repo":
                chunk["base"]["repo"]["full_name"] = "other/repository"
            elif mutation == "main":
                chunk["base"]["ref"] = "main"
            elif mutation == "closed":
                chunk["state"] = "closed"
            else:
                chunk["merged"] = True
            with (
                self.subTest(mutation=mutation),
                self.assertRaises(coverage.CoverageError),
            ):
                coverage.prove(api, NUMBER, HEAD)


class CompletedBoundedReviews(unittest.TestCase):
    def test_completed_chunk_reviews_without_formal_review_nodes_allow_full_coverage(
        self,
    ):
        api = Payloads()
        verdict = gate.verify(api, NUMBER, expected_head=HEAD, publish=True)
        self.assertEqual((verdict.head, verdict.state), (HEAD, "success"))
        self.assertEqual([v.state for v in api.published], ["pending", "success"])
        self.assertEqual(
            api.history[-2:], [("pull", NUMBER), ("publish", HEAD, "success")]
        )
        final_reads = [row for row in api.history if row[0] == "pull"][-4:]
        self.assertEqual([row[1] for row in final_reads], [NUMBER, *BATCHES, NUMBER])

    def test_missing_pending_generic_skip_and_impersonated_target_never_use_metadata(
        self,
    ):
        for rows in [
            [],
            [review(HEAD, state="pending")],
            [review(HEAD, description="Review skipped")],
            [
                review(
                    HEAD,
                    description="Review skipped: 301 files exceed the limit of 300",
                    creator={"login": "coderabbitai[bot]", "id": 1, "type": "Bot"},
                )
            ],
        ]:
            api = Payloads()
            api.status_versions[HEAD] = [rows]
            api.pull_versions[NUMBER][0]["body"] = "malformed yamaa-review-batches"
            with self.subTest(rows=rows):
                self.assertNotEqual(gate.verify(api, NUMBER).state, "success")
                self.assertFalse(any(row[0] == "request" for row in api.history))

    def test_an_ordinary_completed_target_needs_no_batch_metadata_or_review_nodes(self):
        api = Payloads()
        api.status_versions[HEAD] = [[review(HEAD)]]
        api.pull_versions[NUMBER][0]["body"] = "malformed yamaa-review-batches"
        self.assertEqual(gate.verify(api, NUMBER).state, "success")
        self.assertFalse(any(row[0] == "request" for row in api.history))

    def test_no_metadata_cannot_turn_a_large_skip_into_review_completion(self):
        api = Payloads()
        api.pull_versions[NUMBER][0]["body"] = "ordinary description"
        self.assertEqual(gate.verify(api, NUMBER).state, "failure")
        self.assertFalse(any(row[0] == "request" for row in api.history))

    def test_each_chunk_needs_authentic_explicit_completion(self):
        head = CHUNK_HEADS[0]
        for rows, expected in [
            ([], "pending"),
            ([review(head, state="pending")], "pending"),
            ([review(head, description="Review skipped")], "failure"),
            ([review(head, creator={})], "pending"),
            ([review(head, state="failure")], "failure"),
        ]:
            api = Payloads()
            api.status_versions[head] = [rows]
            with self.subTest(rows=rows):
                self.assertEqual(gate.verify(api, NUMBER).state, expected)

    def test_unresolved_findings_in_either_target_or_chunk_block(self):
        for number in [NUMBER, *BATCHES]:
            api = Payloads()
            api.thread_versions[number] = [[{"id": "unresolved", "isResolved": False}]]
            with self.subTest(number=number):
                self.assertEqual(gate.verify(api, NUMBER).state, "failure")

    def test_target_or_chunk_review_restarting_invalidates_bounded_success(self):
        for head in [HEAD, *CHUNK_HEADS]:
            api = Payloads()
            current = api.status_versions[head][0]
            api.status_versions[head] = (
                [current, [review(head, identity=8, state="pending")]]
                if head == HEAD
                else [current, current, [review(head, identity=8, state="pending")]]
            )
            with self.subTest(head=head):
                self.assertEqual(
                    gate.verify(api, NUMBER, publish=True).state, "pending"
                )
                self.assertNotIn("success", [v.state for v in api.published])

    def test_final_head_base_and_metadata_races_never_publish_success(self):
        for number in [NUMBER, *BATCHES]:
            for field in ["head", "base", "head-ref", "base-ref", "metadata", "count"]:
                if field == "metadata" and number != NUMBER:
                    continue
                api = Payloads()
                original = api.pull_versions[number][0]
                moved = copy.deepcopy(original)
                if field in {"head", "base"}:
                    moved[field]["sha"] = oid(98765)
                elif field in {"head-ref", "base-ref"}:
                    moved[field.removesuffix("-ref")]["ref"] += "-changed"
                elif field == "metadata":
                    moved["body"] = "<!-- yamaa-review-batches: [102, 101] -->"
                else:
                    moved["changed_files"] += 1
                api.pull_versions[number] = (
                    [original, original, moved, moved]
                    if number == NUMBER
                    else [original, moved]
                )
                with (
                    self.subTest(number=number, field=field),
                    self.assertRaises(gate.GateError),
                ):
                    gate.verify(api, NUMBER, publish=True)
                self.assertNotIn("success", [v.state for v in api.published])

    def test_invalid_tree_coverage_publishes_failure_instead_of_leaving_old_success(
        self,
    ):
        api = Payloads((range(150), range(151, 301)))
        with self.assertRaises(gate.GateError):
            gate.verify(api, NUMBER, publish=True)
        self.assertEqual([v.state for v in api.published], ["pending", "failure"])


class ReviewScope(unittest.TestCase):
    def test_other_pr_associations_even_with_moved_heads_cannot_supply_the_status(self):
        for final_only in [False, True]:
            api = Payloads()
            original_pages, reads = api.pages, []

            def pages(endpoint):
                rows = original_pages(endpoint)
                if endpoint == f"commits/{CHUNK_HEADS[0]}/pulls":
                    reads.append(endpoint)
                    if not final_only or len(reads) == 2:
                        other = copy.deepcopy(rows[0])
                        other["number"] = 999
                        other["state"] = "closed"
                        other["head"]["sha"] = ORIGINAL
                        rows.append(other)
                return rows

            api.pages = pages
            with (
                self.subTest(final_only=final_only),
                self.assertRaisesRegex(gate.GateError, "exclusively"),
            ):
                gate.verify(api, NUMBER, publish=True)
            self.assertNotIn("success", [v.state for v in api.published])

    def test_the_synthetic_base_and_head_have_the_exact_immutable_parents(self):
        for commit, parents in [
            (CHUNK_HEADS[0], [HEAD]),
            (CHUNK_BASES[0], [ORIGINAL]),
            (CHUNK_HEADS[0], [CHUNK_BASES[0], HEAD]),
            (CHUNK_BASES[0], []),
        ]:
            api = Payloads()
            api.requests[f"git/commits/{commit}"]["parents"] = [
                {"sha": parent} for parent in parents
            ]
            with (
                self.subTest(commit=commit, parents=parents),
                self.assertRaisesRegex(coverage.CoverageError, "synthetic base child"),
            ):
                coverage.prove(api, NUMBER, HEAD)

    def test_any_base_retarget_force_push_or_automatic_change_requires_fresh_review_pr(
        self,
    ):
        for event in [
            "base_ref_changed",
            "base_ref_force_pushed",
            "automatic_base_change_succeeded",
            "automatic_base_change_failed",
        ]:
            api = Payloads()
            api.timeline_versions[BATCHES[0]][0].append({"event": event})
            with (
                self.subTest(event=event),
                self.assertRaisesRegex(coverage.CoverageError, "base history changed"),
            ):
                coverage.prove(api, NUMBER, HEAD)

    def test_missing_or_partial_timeline_is_not_evidence_of_an_unchanged_base(self):
        for events in [[], [{"event": "committed", "sha": HEAD}], [{"event": None}]]:
            api = Payloads()
            api.timeline_versions[BATCHES[0]] = [events]
            with self.subTest(events=events), self.assertRaises(coverage.CoverageError):
                coverage.prove(api, NUMBER, HEAD)

    def test_base_history_on_a_second_api_page_cannot_be_omitted(self):
        source = Payloads()
        chunk = coverage.snapshot(source, BATCHES[0], target=False)
        api = object.__new__(gate.GitHub)
        api.repository, api.repository_id = REPOSITORY, REPOSITORY_ID
        first = [{"event": "committed", "sha": chunk.head}] + [
            {"event": "commented"} for _ in range(99)
        ]
        api.request = Mock(
            side_effect=[
                [source.pull_versions[chunk.number][0]],
                first,
                [{"event": "base_ref_force_pushed"}],
            ]
        )
        with self.assertRaisesRegex(coverage.CoverageError, "base history changed"):
            coverage.check_binding(api, chunk)
        self.assertTrue(
            api.request.call_args_list[-1]
            .args[1]
            .endswith("timeline?per_page=100&page=2")
        )

    def test_authentic_walkthrough_after_100_other_comments_is_read(self):
        source = Payloads()
        chunk = coverage.snapshot(source, BATCHES[0], target=False)
        api = object.__new__(gate.GitHub)
        api.repository, api.repository_id = REPOSITORY, REPOSITORY_ID
        api.request = Mock(
            side_effect=[
                [{"user": {"login": "untrusted"}} for _ in range(100)],
                source.comment_versions[chunk.number][0],
            ]
        )
        coverage.attest_completed_range(
            api, chunk, source.status_versions[chunk.head][0][0], gate.authentic
        )
        self.assertTrue(
            api.request.call_args_list[-1]
            .args[1]
            .endswith("comments?per_page=100&page=2")
        )

    def test_a_retarget_during_verification_invalidates_success(self):
        api = Payloads()
        initial = api.timeline_versions[BATCHES[0]][0]
        api.timeline_versions[BATCHES[0]] = [
            initial,
            [*initial, {"event": "base_ref_changed"}],
        ]
        with self.assertRaisesRegex(gate.GateError, "base history changed"):
            gate.verify(api, NUMBER, publish=True)
        self.assertNotIn("success", [v.state for v in api.published])

    def test_authentic_walkthrough_must_explicitly_name_this_base_and_head(self):
        for mutation in [
            "old-base",
            "old-head",
            "fake-user",
            "missing",
            "wrong-issue",
            "no-footer",
            "skipped-footer",
        ]:
            api = Payloads()
            comment = api.comment_versions[BATCHES[0]][0][0]
            if mutation == "old-base":
                comment["body"] = comment["body"].replace(CHUNK_BASES[0], ORIGINAL)
            elif mutation == "old-head":
                comment["body"] = comment["body"].replace(CHUNK_HEADS[0], HEAD)
            elif mutation == "fake-user":
                comment["user"]["id"] = 1
            elif mutation == "missing":
                api.comment_versions[BATCHES[0]] = [[]]
            elif mutation == "wrong-issue":
                comment["issue_url"] = (
                    f"https://api.github.com/repos/{REPOSITORY}/issues/999"
                )
            elif mutation == "no-footer":
                comment["body"] = comment["body"].split("\n")[0]
            else:
                comment["body"] = comment["body"].replace('"reviewed"', '"skipped"')
            with self.subTest(mutation=mutation), self.assertRaises(gate.GateError):
                gate.verify(api, NUMBER, publish=True)
            self.assertNotIn("success", [v.state for v in api.published])

    def test_a_walkthrough_updated_after_completion_cannot_borrow_that_old_status(self):
        api = Payloads()
        api.comment_versions[BATCHES[0]][0][0]["updated_at"] = "2026-10-15T14:16:06Z"
        with self.assertRaisesRegex(gate.GateError, "completion predates"):
            gate.verify(api, NUMBER)

    def test_latest_authentic_walkthrough_wins_instead_of_an_older_matching_range(self):
        api = Payloads()
        old = api.comment_versions[BATCHES[0]][0][0]
        newer = copy.deepcopy(old)
        newer["id"] += 100
        newer["updated_at"] = "2026-10-15T14:16:04Z"
        newer["body"] = newer["body"].replace(CHUNK_BASES[0], ORIGINAL)
        api.comment_versions[BATCHES[0]][0].append(newer)
        with self.assertRaisesRegex(gate.GateError, "different base or head"):
            gate.verify(api, NUMBER)

    def test_normal_base_fast_forward_still_needs_a_completed_review_of_the_final_base(
        self,
    ):
        # A normal fast-forward has no retarget/force-push timeline event.
        # The exact app-authored reviewed range must independently bind it.
        api = Payloads()
        comment = api.comment_versions[BATCHES[0]][0][0]
        comment["body"] = comment["body"].replace(CHUNK_BASES[0], ORIGINAL)
        with self.assertRaisesRegex(gate.GateError, "different base or head"):
            gate.verify(api, NUMBER)

    def test_status_change_while_reading_the_walkthrough_invalidates_success(self):
        api = Payloads()
        head = CHUNK_HEADS[0]
        current = api.status_versions[head][0]
        api.status_versions[head] = [
            current,
            current,
            current,
            [review(head, identity=8, state="pending")],
        ]
        self.assertEqual(gate.verify(api, NUMBER).state, "pending")

    def test_malformed_footer_timestamps_and_repeated_walkthroughs_fail_closed(self):
        for mutation in [
            "timestamp",
            "status-time",
            "json",
            "duplicate-footer",
            "duplicate-comment",
        ]:
            api = Payloads()
            comment = api.comment_versions[BATCHES[0]][0][0]
            if mutation == "timestamp":
                comment["updated_at"] = "unknown"
            elif mutation == "status-time":
                api.status_versions[CHUNK_HEADS[0]][0][0].pop("created_at")
            elif mutation == "json":
                comment["body"] = comment["body"].replace(
                    '"kind":"reviewed"', '"kind":'
                )
            elif mutation == "duplicate-footer":
                comment["body"] += "\n" + comment["body"].split("\n")[1]
            else:
                api.comment_versions[BATCHES[0]][0].append(copy.deepcopy(comment))
            with self.subTest(mutation=mutation), self.assertRaises(gate.GateError):
                gate.verify(api, NUMBER)


class BoundedRouting(unittest.TestCase):
    @staticmethod
    def event(**fields):
        return {"repository": dict(REPO), **fields}

    def test_chunk_status_refreshes_its_explicitly_linked_main_target(self):
        api = Payloads()
        self.assertEqual(
            gate.candidates(
                api,
                "status",
                self.event(
                    context="CodeRabbit", sender=gate.REVIEW_CREATOR, sha=CHUNK_HEADS[0]
                ),
            ),
            [NUMBER],
        )
        self.assertIn(("pages", "pulls?state=open&base=main"), api.history)

    def test_chunk_discussion_refreshes_both_itself_and_linked_main_target(self):
        api = Payloads()
        self.assertEqual(
            gate.candidates(
                api,
                "issue_comment",
                self.event(
                    issue={
                        "number": BATCHES[0],
                        "pull_request": {"url": "not followed"},
                    }
                ),
            ),
            [NUMBER, BATCHES[0]],
        )

    def test_own_status_still_never_recurses_or_reads_pr_metadata(self):
        api = Payloads()
        self.assertEqual(
            gate.candidates(
                api,
                "status",
                self.event(
                    context=gate.CONTEXT, sender=gate.REVIEW_CREATOR, sha=CHUNK_HEADS[0]
                ),
            ),
            [],
        )
        self.assertEqual(api.history, [])


if __name__ == "__main__":
    unittest.main()
