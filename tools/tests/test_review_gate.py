"""Review coverage belongs to the current source, authentic app and full threads."""

import copy
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

from tools.ci import review_gate as gate

REPOSITORY = "elong0527/yamaa"
REPOSITORY_ID = 1299533902
HEAD, OTHER = "a" * 40, "b" * 40
NUMBER = 42
EVENT_REPOSITORY = {"full_name": REPOSITORY, "id": REPOSITORY_ID}


def pull(head=HEAD, *, branch="main", state="open", repository_id=REPOSITORY_ID):
    return {
        "number": NUMBER,
        "state": state,
        "base": {"ref": branch, "repo": {"full_name": REPOSITORY, "id": repository_id}},
        "head": {
            "sha": head,
            "ref": "never executed",
            "repo": {"full_name": "contributor/fork"},
        },
    }


def status(
    identity=7,
    *,
    head=HEAD,
    state="success",
    description="Review completed",
    **overrides,
):
    # The account identity and exact completed text are from the authentic app
    # status on merged clean PR #2465, which has no formal PR review nodes.
    result = {
        "id": identity,
        "state": state,
        "description": description,
        "context": "CodeRabbit",
        "creator": {"login": "coderabbitai[bot]", "id": 136622811, "type": "Bot"},
        "url": f"https://api.github.com/repos/{REPOSITORY}/statuses/{head}",
    }
    result.update(overrides)
    return result


def thread(identity="T1", resolved=True):
    return {"id": identity, "isResolved": resolved}


class Evidence:
    repository = REPOSITORY
    repository_id = REPOSITORY_ID

    def __init__(self, *, statuses=None, threads=None, pulls=None):
        self.status_snapshots = statuses if statuses is not None else [[status()]]
        self.thread_snapshots = threads if threads is not None else [[]]
        self.pull_snapshots = pulls if pulls is not None else [pull()]
        self.history, self.published = [], []
        self.pages = Mock(return_value=[])

    @staticmethod
    def next(snapshots):
        return copy.deepcopy(snapshots.pop(0) if len(snapshots) > 1 else snapshots[0])

    def pull(self, number):
        self.history.append(("pull", number))
        return self.next(self.pull_snapshots)

    def statuses(self, head):
        self.history.append(("statuses", head))
        return self.next(self.status_snapshots)

    def threads(self, number, head):
        self.history.append(("threads", number, head))
        return self.next(self.thread_snapshots)

    def publish(self, verdict):
        self.history.append(("publish", verdict.head, verdict.state))
        self.published.append(verdict)


class MultipleEvidence(Evidence):
    """A malformed large PR followed by a clean independently completed PR."""

    def __init__(self):
        super().__init__()
        first, second = pull(), pull(OTHER)
        first.update(
            merged=False,
            changed_files=301,
            body="<!-- yamaa-review-batches: [invalid JSON] -->",
        )
        first["base"]["sha"] = OTHER
        second["number"] = NUMBER + 1
        self.pulls = {NUMBER: first, NUMBER + 1: second}
        self.pages = Mock(return_value=list(self.pulls.values()))

    def pull(self, number):
        self.history.append(("pull", number))
        return copy.deepcopy(self.pulls[number])

    def statuses(self, head):
        self.history.append(("statuses", head))
        return [
            status(
                head=head,
                description=(
                    "Review skipped: 301 files exceed the limit of 300"
                    if head == HEAD
                    else "Review completed"
                ),
            )
        ]


def client(responses):
    api = object.__new__(gate.GitHub)
    api.repository, api.repository_id = REPOSITORY, REPOSITORY_ID
    api.request = Mock(side_effect=responses)
    return api


def thread_page(nodes, total, *, cursor=None, more=False, head=HEAD, errors=None):
    value = {
        "data": {
            "repository": {
                "pullRequest": {
                    "number": NUMBER,
                    "headRefOid": head,
                    "reviewThreads": {
                        "totalCount": total,
                        "nodes": nodes,
                        "pageInfo": {"hasNextPage": more, "endCursor": cursor},
                    },
                }
            }
        }
    }
    if errors is not None:
        value["errors"] = errors
    return value


class ExactCoverage(unittest.TestCase):
    def test_clean_completed_head_does_not_need_a_formal_review_node_or_write(self):
        api = Evidence(threads=[[thread()]])
        verdict = gate.verify(api, NUMBER, expected_head=HEAD)
        self.assertEqual((verdict.head, verdict.state), (HEAD, "success"))
        self.assertEqual(api.published, [])
        self.assertEqual(
            [row[0] for row in api.history],
            ["pull", "statuses", "threads", "statuses", "pull"],
        )

    def test_old_review_cannot_replace_pending_or_missing_final_head_status(self):
        for rows in [[], [status(state="pending", description="Review in progress")]]:
            api = Evidence(statuses=[rows])
            api.formal_reviews = [{"commit_id": OTHER, "state": "COMMENTED"}]
            self.assertEqual(gate.verify(api, NUMBER).state, "pending")
            self.assertFalse(any(row[0] == "threads" for row in api.history))

    def test_successful_skips_and_unknown_completion_text_are_not_completed_reviews(
        self,
    ):
        for description in [
            None,
            "",
            "Review skipped",
            "Review skipped: 300-file limit",
            "Review completed with skipped files",
        ]:
            with self.subTest(description=description):
                self.assertEqual(
                    gate.verify(
                        Evidence(statuses=[[status(description=description)]]), NUMBER
                    ).state,
                    "failure",
                )

    def test_newest_authentic_status_wins_even_if_rows_are_not_ordered(self):
        for state in ["pending", "failure", "error"]:
            api = Evidence(
                statuses=[
                    [
                        status(30, state=state),
                        status(100, context=gate.CONTEXT),
                        status(7),
                    ]
                ]
            )
            self.assertEqual(
                gate.verify(api, NUMBER).state,
                "pending" if state == "pending" else "failure",
            )

    def test_impersonated_context_account_or_account_type_never_proves_completion(self):
        for creator in [
            None,
            {},
            {"login": "coderabbitai[bot]", "id": 123, "type": "Bot"},
            {"login": "other[bot]", "id": 136622811, "type": "Bot"},
            {"login": "coderabbitai[bot]", "id": 136622811, "type": "User"},
        ]:
            with self.subTest(creator=creator):
                forged = status(100, creator=creator)
                self.assertEqual(
                    gate.verify(Evidence(statuses=[[forged]]), NUMBER).state, "pending"
                )
                self.assertEqual(
                    gate.verify(
                        Evidence(statuses=[[forged, status(state="pending")]]), NUMBER
                    ).state,
                    "pending",
                )

    def test_status_from_another_head_and_invalid_authentic_identity_refuse(self):
        for row in [status(head=OTHER), status(identity=None), status(identity=True)]:
            with self.subTest(row=row), self.assertRaises(gate.GateError):
                gate.verify(Evidence(statuses=[[row]]), NUMBER)

    def test_any_unresolved_thread_blocks_even_when_outdated(self):
        for outdated in [False, True]:
            row = thread(resolved=False)
            row["isOutdated"] = outdated
            self.assertEqual(
                gate.verify(Evidence(threads=[[thread(), row]]), NUMBER).state,
                "failure",
            )

    def test_manual_expected_head_is_checked_before_any_evidence_or_status_write(self):
        api = Evidence()
        with self.assertRaisesRegex(gate.GateError, "stale"):
            gate.verify(api, NUMBER, expected_head=OTHER, publish=True)
        self.assertEqual(api.history, [("pull", NUMBER)])

    def test_changed_head_never_receives_a_stale_success(self):
        api = Evidence(pulls=[pull(), pull(OTHER), pull(OTHER)])
        with self.assertRaisesRegex(gate.GateError, "head changed"):
            gate.verify(api, NUMBER, publish=True)
        self.assertEqual(
            [(v.head, v.state) for v in api.published],
            [(HEAD, "pending"), (OTHER, "failure")],
        )
        self.assertEqual(
            api.history[-2:], [("pull", NUMBER), ("publish", OTHER, "failure")]
        )

    def test_success_publication_follows_a_fresh_pull_head_read(self):
        api = Evidence()
        self.assertEqual(gate.verify(api, NUMBER, publish=True).state, "success")
        self.assertEqual([v.state for v in api.published], ["pending", "success"])
        self.assertEqual(
            api.history[-2:], [("pull", NUMBER), ("publish", HEAD, "success")]
        )

    def test_review_restarting_during_thread_read_invalidates_old_completion(self):
        api = Evidence(statuses=[[status(7)], [status(8, state="pending")]])
        self.assertEqual(gate.verify(api, NUMBER, publish=True).state, "pending")
        self.assertNotIn("success", [v.state for v in api.published])

    def test_new_completed_review_requires_reading_its_new_unresolved_threads(self):
        api = Evidence(
            statuses=[[status(7)], [status(8)]],
            threads=[[thread()], [thread(resolved=False)]],
        )
        self.assertEqual(gate.verify(api, NUMBER).state, "failure")
        self.assertEqual(sum(row[0] == "threads" for row in api.history), 2)

    def test_perpetually_changing_review_fails_closed_instead_of_using_a_snapshot(self):
        api = Evidence(statuses=[[status(index)] for index in range(1, 7)])
        self.assertEqual(gate.verify(api, NUMBER).state, "pending")

    def test_unavailable_thread_evidence_invalidates_the_published_context(self):
        api = Evidence()
        api.threads = Mock(side_effect=gate.GateError("thread evidence unavailable"))
        with self.assertRaises(gate.GateError):
            gate.verify(api, NUMBER, publish=True)
        self.assertEqual([v.state for v in api.published], ["pending", "failure"])

    def test_closed_wrong_repository_and_non_main_pulls_do_not_publish(self):
        for value in [
            pull(state="closed"),
            pull(branch="release"),
            pull(repository_id=123),
        ]:
            api = Evidence(pulls=[value])
            self.assertIsNone(gate.verify(api, NUMBER, publish=True))
            self.assertEqual(api.published, [])


class CompletePagination(unittest.TestCase):
    def test_authentic_status_after_a_full_page_is_read_and_url_is_exact(self):
        unrelated = [status(index, context="CI") for index in range(1, 101)]
        api = client([unrelated, [status(300)]])
        self.assertEqual(
            gate.latest_review(api, HEAD), (300, "success", "Review completed")
        )
        self.assertEqual(
            [call.args[1] for call in api.request.call_args_list],
            [
                f"repos/{REPOSITORY}/commits/{HEAD}/statuses?per_page=100&page=1",
                f"repos/{REPOSITORY}/commits/{HEAD}/statuses?per_page=100&page=2",
            ],
        )

    def test_a_full_last_rest_page_requires_the_terminal_page(self):
        api = client([[pull() for _ in range(100)], []])
        self.assertEqual(len(api.pages("pulls?state=open&base=main")), 100)
        self.assertEqual(
            api.request.call_args_list[-1].args[1],
            f"repos/{REPOSITORY}/pulls?state=open&base=main&per_page=100&page=2",
        )

    def test_unresolved_thread_on_the_second_page_is_not_lost(self):
        first = [thread(f"T{index}") for index in range(100)]
        api = client(
            [
                thread_page(first, 101, cursor="next", more=True),
                thread_page([thread("LAST", False)], 101),
            ]
        )
        rows = api.threads(NUMBER, HEAD)
        self.assertEqual(len(rows), 101)
        self.assertFalse(rows[-1]["isResolved"])
        self.assertIsNone(api.request.call_args_list[0].args[2]["variables"]["cursor"])
        self.assertEqual(
            api.request.call_args_list[1].args[2]["variables"]["cursor"], "next"
        )

    def test_partial_malformed_or_changing_thread_payloads_refuse(self):
        for responses in [
            [thread_page([], 0, errors=[{"message": "partial"}])],
            [thread_page([thread()], 2)],
            [thread_page([{"id": "T", "isResolved": None}], 1)],
            [
                thread_page([thread()], 2, cursor="same", more=True),
                thread_page([thread("T2")], 2, cursor="same", more=True),
            ],
            [
                thread_page([thread()], 2, cursor="next", more=True),
                thread_page([thread()], 2),
            ],
            [
                thread_page([thread()], 2, cursor="next", more=True),
                thread_page([thread("T2")], 3),
            ],
            [
                thread_page([thread()], 2, cursor="next", more=True),
                thread_page([thread("T2")], 2, head=OTHER),
            ],
        ]:
            with self.subTest(responses=responses), self.assertRaises(gate.GateError):
                client(responses).threads(NUMBER, HEAD)

    def test_missing_or_malformed_rest_pages_are_not_empty_evidence(self):
        for response in [{"items": []}, None, [pull() for _ in range(101)]]:
            with self.subTest(response=response), self.assertRaises(gate.GateError):
                client([response]).pages("pulls")

    def test_repeated_full_rest_page_refuses_instead_of_looping(self):
        page = [status(index) for index in range(1, 101)]
        with self.assertRaisesRegex(gate.GateError, "did not advance"):
            client([page, copy.deepcopy(page)]).pages("commits/current/statuses")


class TrustedRouting(unittest.TestCase):
    def event(self, **fields):
        return {"repository": dict(EVENT_REPOSITORY), **fields}

    def test_own_status_and_impersonated_bot_events_do_not_recurse(self):
        api = Evidence()
        for event in [
            self.event(context=gate.CONTEXT, sender=gate.REVIEW_CREATOR, sha=HEAD),
            self.event(
                context="CodeRabbit",
                sender={"login": "coderabbitai[bot]", "id": 7, "type": "Bot"},
                sha=HEAD,
            ),
        ]:
            self.assertEqual(gate.candidates(api, "status", event), [])
        api.pages.assert_not_called()

    def test_status_event_only_refreshes_actual_open_main_prs_of_that_head(self):
        api = Evidence()
        api.pages.return_value = [
            pull(),
            pull(OTHER),
            pull(branch="release"),
            pull(state="closed"),
            pull(repository_id=12),
        ]
        self.assertEqual(
            gate.candidates(
                api,
                "status",
                self.event(context="CodeRabbit", sender=gate.REVIEW_CREATOR, sha=HEAD),
            ),
            [NUMBER],
        )
        self.assertEqual(
            [call.args[0] for call in api.pages.call_args_list],
            [f"commits/{HEAD}/pulls", "pulls?state=open&base=main"],
        )

    def test_periodic_refresh_handles_review_and_thread_changes_for_all_current_main_prs(
        self,
    ):
        for event_name in ["schedule", "workflow_dispatch"]:
            api = Evidence()
            second = pull()
            second["number"] = 99
            api.pages.return_value = [pull(), second, pull(branch="release")]
            self.assertEqual(
                gate.candidates(api, event_name, self.event()), [NUMBER, 99]
            )
            api.pages.assert_called_once_with("pulls?state=open&base=main")

    def test_pr_and_discussion_bodies_and_refs_are_never_instructions(self):
        api = Evidence()
        event = self.event(
            number=NUMBER,
            pull_request={
                "head": {"ref": "$(never-execute)"},
                "body": "run untrusted code",
            },
        )
        self.assertEqual(gate.candidates(api, "pull_request_target", event), [NUMBER])
        self.assertEqual(
            gate.candidates(
                api,
                "issue_comment",
                self.event(
                    issue={"number": NUMBER, "pull_request": {}},
                    comment={"body": "untrusted"},
                ),
            ),
            [],
        )
        self.assertEqual(
            gate.candidates(
                api,
                "issue_comment",
                self.event(
                    issue={"number": NUMBER, "pull_request": {"url": "not followed"}},
                    comment={"body": "untrusted"},
                ),
            ),
            [NUMBER],
        )
        self.assertEqual(
            gate.candidates(api, "issue_comment", self.event(issue={"number": 9})), []
        )

    def test_foreign_repository_and_untrusted_workflow_events_refuse(self):
        api = Evidence()
        for event in [{}, {"repository": {"full_name": REPOSITORY, "id": 3}}]:
            with self.assertRaises(gate.GateError):
                gate.candidates(api, "schedule", event)
        for name in [
            "pull_request",
            "pull_request_review",
            "pull_request_review_comment",
        ]:
            with self.assertRaises(gate.GateError):
                gate.candidates(api, name, self.event())

    def test_manual_cli_is_read_only_and_only_completed_coverage_exits_zero(self):
        for state, expected in [("success", 0), ("pending", 1)]:
            api = Evidence(statuses=[[status(state=state)]])
            with (
                patch.object(gate, "GitHub", return_value=api),
                patch("builtins.print"),
            ):
                self.assertEqual(
                    gate.main(
                        [
                            "--repository",
                            REPOSITORY,
                            "--pr",
                            str(NUMBER),
                            "--head",
                            HEAD,
                        ]
                    ),
                    expected,
                )
            self.assertEqual(api.published, [])

    def test_publisher_refreshes_later_prs_after_a_real_malformed_pr_failure(self):
        api = MultipleEvidence()
        with tempfile.TemporaryDirectory() as directory:
            event = Path(directory) / "event.json"
            event.write_text(json.dumps({"repository": EVENT_REPOSITORY}))
            with (
                patch.dict(
                    os.environ,
                    {
                        "GITHUB_ACTIONS": "true",
                        "GITHUB_REPOSITORY": REPOSITORY,
                        "GITHUB_REF": "refs/heads/main",
                        "GITHUB_EVENT_NAME": "workflow_dispatch",
                        "GITHUB_EVENT_PATH": str(event),
                    },
                    clear=True,
                ),
                patch.object(gate, "GitHub", return_value=api),
                patch("builtins.print") as output,
            ):
                self.assertEqual(gate.main(["--publish"]), 1)
        # Neither the candidate selector nor verify/publication is mocked:
        # the first PR publishes a failure and the second still gets success.
        self.assertEqual(
            [(verdict.head, verdict.state) for verdict in api.published],
            [
                (HEAD, "pending"),
                (HEAD, "failure"),
                (OTHER, "pending"),
                (OTHER, "success"),
            ],
        )
        self.assertIn(("threads", NUMBER + 1, OTHER), api.history)
        self.assertTrue(
            any(
                f"PR #{NUMBER}: invalid review-batch JSON" in call.args[0]
                for call in output.call_args_list
            )
        )

    def test_read_only_cli_still_raises_immediately_for_the_same_invalid_pr(self):
        api = MultipleEvidence()
        with (
            patch.object(gate, "GitHub", return_value=api),
            self.assertRaisesRegex(gate.GateError, "invalid review-batch JSON"),
        ):
            gate.main(["--repository", REPOSITORY, "--pr", str(NUMBER), "--head", HEAD])
        self.assertEqual(api.published, [])
        self.assertNotIn(("pull", NUMBER + 1), api.history)

    def test_publication_cannot_be_enabled_from_a_normal_local_command(self):
        with (
            patch.dict(os.environ, {}, clear=True),
            patch.object(gate, "GitHub") as constructor,
        ):
            with self.assertRaises(gate.GateError):
                gate.main(["--repository", REPOSITORY, "--publish"])
            constructor.assert_not_called()

    def test_manual_workflow_publication_is_restricted_to_the_default_branch(self):
        with (
            patch.dict(
                os.environ,
                {
                    "GITHUB_ACTIONS": "true",
                    "GITHUB_REPOSITORY": REPOSITORY,
                    "GITHUB_REF": "refs/heads/untrusted-change",
                },
                clear=True,
            ),
            patch.object(gate, "GitHub") as constructor,
        ):
            with self.assertRaises(gate.GateError):
                gate.main(["--repository", REPOSITORY, "--publish"])
            constructor.assert_not_called()

    def test_api_errors_do_not_echo_authentication_or_untrusted_payloads(self):
        api = object.__new__(gate.GitHub)
        result = subprocess.CompletedProcess(
            [], 1, "untrusted payload", "not-a-real-token"
        )
        with patch.object(gate.subprocess, "run", return_value=result) as command:
            with self.assertRaisesRegex(gate.GateError, "^GitHub API request failed$"):
                api.request("GET", f"repos/{REPOSITORY}")
        self.assertNotIn("not-a-real-token", str(command.call_args))
        self.assertNotIn("untrusted payload", str(command.call_args))


if __name__ == "__main__":
    unittest.main()
