#!/usr/bin/env python3
"""Fictional GitHub API for Subspace debug screenshots.

Usage: python3 scripts/github_test_server.py [--port N] [--runs-delay SECONDS]

Port 0 picks a free port. The first line on stdout names the address as
SUBSPACE_GITHUB_API_URL=http://127.0.0.1:<port>. scripts/capture.sh starts
it for every capture. --runs-delay holds each repository's Actions run list
for that many seconds, so a capture can show the Actions board while it loads
(capture.sh passes CAPTURE_RUNS_DELAY).

Answers the GraphQL and REST requests of the GitHub (PRs), Copilot Agents and
Actions boards with invented data: the organisation `acme`, the repositories
acme/web, acme/api and acme/design-system, and the people octo-dev, mona,
hubot, lin-dev, sam-codes and ada-ops. Point a debug build at it with
SUBSPACE_GITHUB_API_URL=http://127.0.0.1:<port>.

The token selects the viewer: `fixture-<login>` (the capture script's fake
gh CLI hands these out) answers as <login>; any other token is octo-dev.
Every request is logged to stderr; unknown ones are marked UNKNOWN and get
an empty but valid answer.

It also stands in for Sign in with GitHub when a debug build sets
SUBSPACE_GITHUB_OAUTH_URL to the same address: /login/oauth/authorize
redirects straight back to a loopback redirect_uri, /login/device/code hands
out the code OCTO-1234, /login/oauth/access_token answers both grants with
the token fixture-mona (scopes repo and read:org; the device grant after two
pending polls), and DELETE /applications/<id>/grant revokes nothing.
"""

import argparse
import hashlib
import json
import re
import sys
import time
from datetime import datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, quote, urlencode, urlparse

VIEWER = "octo-dev"
WORK_VIEWER = "octo-dev_acme"
NOW = datetime.now(timezone.utc).replace(microsecond=0)
BASE_URL = "http://127.0.0.1"
RUNS_DELAY = 0.0

PEOPLE = {
    "octo-dev": "Octo Developer",
    "octo-dev_acme": "Octo Developer",
    "mona": "Mona Lisa Octocat",
    "hubot": "Hubot",
    "lin-dev": "Lin Park",
    "sam-codes": "Sam Rivera",
    "ada-ops": "Ada Byte",
}

REPOS = {
    "acme/web": 4101,
    "acme/api": 4102,
    "acme/design-system": 4103,
}

LABELS = {
    "bug": "d73a4a",
    "enhancement": "a2eeef",
    "frontend": "1d76db",
    "backend": "5319e7",
    "dependencies": "0366d6",
    "needs-design": "fbca04",
    "performance": "0e8a16",
    "security": "b60205",
    "docs": "0075ca",
    "breaking": "e99695",
}


def ago(**delta):
    return (NOW - timedelta(**delta)).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha(text):
    return hashlib.sha1(text.encode()).hexdigest()


def actor(login):
    return {"login": login, "name": PEOPLE.get(login)}


# ---- Pull requests and issues ------------------------------------------------

# sections: mine, review, involved (PRs); assigned, created, mentioned (issues)
PRS = [
    dict(repo="acme/web", number=482, title="Add keyboard shortcuts to the command palette",
         author="octo-dev", sections=["mine"], draft=False, labels=["enhancement", "frontend"],
         checks="SUCCESS", review="APPROVED", additions=318, deletions=42, head="palette-shortcuts",
         updated=dict(minutes=25), created=dict(days=3), comments=4, assignees=["octo-dev"],
         reviewers=[("mona", "APPROVED"), ("lin-dev", "COMMENTED")]),
    dict(repo="acme/api", number=1177, title="Paginate the audit log endpoint",
         author="octo-dev", sections=["mine"], draft=False, labels=["backend", "performance"],
         checks="FAILURE", review="CHANGES_REQUESTED", additions=204, deletions=88,
         head="audit-log-pages", updated=dict(hours=2), created=dict(days=5), comments=7,
         assignees=["octo-dev"], reviewers=[("sam-codes", "CHANGES_REQUESTED")]),
    dict(repo="acme/design-system", number=96, title="Draft: dark theme tokens for charts",
         author="octo-dev", sections=["mine"], draft=True, labels=["needs-design"],
         checks="PENDING", review=None, additions=57, deletions=3, head="chart-dark-tokens",
         updated=dict(hours=5), created=dict(days=1), comments=0, assignees=[],
         reviewers=[]),
    dict(repo="acme/web", number=479, title="Fix focus ring on disabled buttons",
         author="octo-dev", sections=["mine"], draft=False, labels=["bug", "frontend"],
         checks="SUCCESS", review="REVIEW_REQUIRED", additions=12, deletions=9,
         head="fix-focus-ring", updated=dict(days=1, hours=3), created=dict(days=2), comments=1,
         assignees=[], reviewers=[("ada-ops", "PENDING")]),
    dict(repo="acme/api", number=1182, title="Rotate signing keys without downtime",
         author="mona", sections=["review"], draft=False, labels=["security", "backend"],
         checks="SUCCESS", review="REVIEW_REQUIRED", additions=436, deletions=120,
         head="key-rotation", updated=dict(minutes=50), created=dict(days=2), comments=3,
         assignees=["mona"], reviewers=[("octo-dev", "PENDING"), ("hubot", "COMMENTED")]),
    dict(repo="acme/web", number=485, title="Migrate settings page to the new form components",
         author="lin-dev", sections=["review"], draft=False, labels=["frontend"],
         checks="PENDING", review="REVIEW_REQUIRED", additions=692, deletions=541,
         head="settings-forms", updated=dict(hours=3), created=dict(days=4), comments=9,
         assignees=["lin-dev"], reviewers=[("octo-dev", "PENDING")]),
    dict(repo="acme/design-system", number=101, title="Bump @acme/icons from 3.2.0 to 3.4.1",
         author="hubot", sections=["review"], draft=False, labels=["dependencies"],
         checks="FAILURE", review="REVIEW_REQUIRED", additions=14, deletions=14,
         head="deps/icons-3.4.1", updated=dict(hours=9), created=dict(days=1), comments=1,
         assignees=[], reviewers=[("octo-dev", "PENDING")]),
    dict(repo="acme/web", number=487, title="Show unsaved-changes warning when leaving the editor",
         author="sam-codes", sections=["review"], draft=False, labels=["enhancement", "frontend"],
         checks="SUCCESS", review="REVIEW_REQUIRED", additions=96, deletions=18,
         head="unsaved-warning", updated=dict(hours=4), created=dict(days=1), comments=2,
         assignees=["sam-codes"], reviewers=[("octo-dev", "PENDING"), ("lin-dev", "APPROVED")]),
    dict(repo="acme/api", number=1179, title="Cache feature flag lookups per request",
         author="octo-dev", sections=["mine"], draft=False, labels=["performance", "backend"],
         checks="SUCCESS", review="APPROVED", additions=73, deletions=21, head="flag-cache",
         updated=dict(days=2, hours=4), created=dict(days=4), comments=3, assignees=["octo-dev"],
         reviewers=[("ada-ops", "APPROVED")]),
    dict(repo="acme/api", number=1169, title="Document the webhook retry policy",
         author="sam-codes", sections=["involved"], draft=False, labels=["docs"],
         checks="SUCCESS", review="APPROVED", additions=88, deletions=6, head="webhook-docs",
         updated=dict(hours=6), created=dict(days=6), comments=5, assignees=[],
         reviewers=[("octo-dev", "APPROVED")]),
    dict(repo="acme/web", number=471, title="Lazy-load the analytics dashboard charts",
         author="ada-ops", sections=["involved"], draft=False, labels=["performance", "frontend"],
         checks="SUCCESS", review="CHANGES_REQUESTED", additions=145, deletions=60,
         head="lazy-charts", updated=dict(days=2), created=dict(days=8), comments=12,
         assignees=["ada-ops"], reviewers=[("octo-dev", "CHANGES_REQUESTED"), ("mona", "APPROVED")]),
    dict(repo="acme/design-system", number=88, title="Remove the deprecated Toolbar v1 API",
         author="mona", sections=["involved"], draft=True, labels=["breaking"],
         checks="PENDING", review=None, additions=12, deletions=980, head="drop-toolbar-v1",
         updated=dict(days=3), created=dict(days=12), comments=2, assignees=[],
         reviewers=[]),
]

ISSUES = [
    dict(repo="acme/web", number=490, title="Command palette loses focus after closing a dialog",
         author="lin-dev", sections=["assigned", "mentioned"], labels=["bug", "frontend"],
         updated=dict(hours=1), created=dict(days=1), comments=3, assignees=["octo-dev"]),
    dict(repo="acme/api", number=1185, title="Rate-limit headers missing on 429 responses",
         author="octo-dev", sections=["created", "assigned"], labels=["bug", "backend"],
         updated=dict(hours=7), created=dict(days=2), comments=2, assignees=["octo-dev"]),
    dict(repo="acme/design-system", number=104, title="Spacing scale for dense tables",
         author="mona", sections=["mentioned"], labels=["needs-design", "enhancement"],
         updated=dict(days=2), created=dict(days=9), comments=6, assignees=["mona"]),
]


def node_id(kind, item):
    return f"{kind}_{item['repo'].replace('/', '_')}_{item['number']}"


def item_url(item, kind):
    path = "pull" if kind == "PR" else "issues"
    return f"https://github.com/{item['repo']}/{path}/{item['number']}"


def common(item, kind):
    return {
        "__typename": "PullRequest" if kind == "PR" else "Issue",
        "id": node_id(kind, item),
        "number": item["number"],
        "title": item["title"],
        "url": item_url(item, kind),
        "state": "OPEN",
        "createdAt": ago(**item["created"]),
        "updatedAt": ago(**item["updated"]),
        "repository": {"nameWithOwner": item["repo"]},
        "author": actor(item["author"]),
        "assignees": {"nodes": [actor(login) for login in item["assignees"]]},
        "labels": {"nodes": [{"name": name, "color": LABELS[name]} for name in item["labels"]]},
        "comments": {"totalCount": item["comments"]},
    }


def check_contexts(pr):
    state = pr["checks"]
    lint_done = state != "PENDING"
    runs = [
        ("build", "COMPLETED", "SUCCESS"),
        ("lint", "COMPLETED" if lint_done else "IN_PROGRESS", "SUCCESS" if lint_done else None),
        ("unit-tests", "COMPLETED" if lint_done else "QUEUED",
         {"SUCCESS": "SUCCESS", "FAILURE": "FAILURE"}.get(state)),
        ("e2e (chromium)", "COMPLETED" if lint_done else "QUEUED",
         "SUCCESS" if lint_done else None),
    ]
    nodes = [
        {"__typename": "CheckRun", "name": name, "status": status, "conclusion": conclusion,
         "detailsUrl": f"https://github.com/{pr['repo']}/actions/runs/{7000 + pr['number']}"}
        for name, status, conclusion in runs
    ]
    nodes.append({"__typename": "StatusContext", "context": "preview/deploy",
                  "state": "SUCCESS" if lint_done else "PENDING",
                  "targetUrl": f"https://preview.example.com/{pr['repo'].split('/')[1]}-{pr['number']}"})
    return nodes


def pr_row(pr):
    row = common(pr, "PR")
    row.update({
        "isDraft": pr["draft"],
        "headRefName": pr["head"],
        "baseRefName": "main",
        "commits": {"nodes": [{"commit": {"statusCheckRollup": {"state": pr["checks"]}}}]},
    })
    return row


def section_nodes(section, kind_wanted):
    if kind_wanted == "PR":
        return [pr_row(pr) for pr in PRS if section in pr["sections"]]
    return [common(issue, "ISSUE") for issue in ISSUES if section in issue["sections"]]


SECTION_QUALIFIERS = [
    ("review-requested:@me", "review"),
    ("involves:@me", "involved"),
    ("assignee:@me", "assigned"),
    ("mentions:@me", "mentioned"),
]


def section_from_query(text):
    for qualifier, section in SECTION_QUALIFIERS:
        if qualifier in text:
            return section
    if "author:@me" in text:
        return "mine" if "is:pr" in text else "created"
    return None


def repo_scope(text):
    return {word[len("repo:"):].lower() for word in text.split() if word.startswith("repo:")}


def search(query, variables):
    text = variables.get("query", "")
    section = section_from_query(text)
    scope = repo_scope(text)
    if section is None and scope and "is:pr" in text:
        # All open: every PR in the named repositories.
        items = [pr for pr in PRS if pr["repo"] in scope]
        if "isDraft" in query:
            rows = [pr_row(pr) for pr in items]
        elif "reviewDecision" in query:
            rows = [{"__typename": "PullRequest", "id": node_id("PR", pr),
                     "reviewDecision": pr["review"]} for pr in items]
        else:
            rows = [{"__typename": "PullRequest", "id": node_id("PR", pr),
                     "additions": pr["additions"], "deletions": pr["deletions"]} for pr in items]
        return {"search": {"issueCount": len(rows), "nodes": rows}}
    if section is None:
        return {"search": {"issueCount": 0, "pageInfo": {"hasNextPage": False, "endCursor": None},
                           "nodes": []}}
    kind = "PR" if "is:pr" in text else "ISSUE"
    rows = section_nodes(section, kind)
    if scope:
        # Favourites only: the personal section within the named repositories.
        rows = [row for row in rows if row["repository"]["nameWithOwner"] in scope]
    if "isDraft" not in query and kind == "PR":
        # The enrichment searches: review decisions or diff stats by node id.
        items = [pr for pr in PRS if section in pr["sections"]
                 and (not scope or pr["repo"] in scope)]
        if "reviewDecision" in query:
            rows = [{"__typename": "PullRequest", "id": node_id("PR", pr),
                     "reviewDecision": pr["review"]} for pr in items]
        else:
            rows = [{"__typename": "PullRequest", "id": node_id("PR", pr),
                     "additions": pr["additions"], "deletions": pr["deletions"]} for pr in items]
    return {"search": {"issueCount": len(rows), "nodes": rows}}


def find_item(owner, name, number):
    repo = f"{owner}/{name}".lower()
    for pr in PRS:
        if pr["repo"] == repo and pr["number"] == number:
            return pr, "PR"
    for issue in ISSUES:
        if issue["repo"] == repo and issue["number"] == number:
            return issue, "ISSUE"
    return None, None


PR_BODIES = {
    "palette-shortcuts": (
        "<p>Adds keyboard shortcuts to every command in the palette and shows them beside "
        "each entry.</p><h3>Changes</h3><ul><li>New <code>useShortcut</code> hook with "
        "platform-aware modifiers</li><li>Shortcut hints in palette rows</li><li>Conflicts are "
        "reported in the console during development</li></ul><p>Closes #470.</p>"
    ),
}


def body_html(item, kind):
    if kind == "PR" and item["head"] in PR_BODIES:
        return PR_BODIES[item["head"]]
    if kind == "PR":
        return (f"<p>{item['title']}.</p><h3>Why</h3><p>This came up in the last planning "
                "session; the current behaviour surprises people and costs support time.</p>"
                "<h3>Testing</h3><ul><li>Unit tests updated</li><li>Checked manually in the "
                "preview deployment</li></ul>")
    return (f"<p>{item['title']}.</p><p><strong>Steps to reproduce</strong></p><ol><li>Open "
            "the app</li><li>Follow the flow described above</li></ol><p>Expected: it keeps "
            "working. Actual: it does not.</p>")


COMMENTS = [
    ("mona", "Thanks for picking this up! One question about the naming, otherwise looks good."),
    ("lin-dev", "I tried this in the preview deployment and it works well for me."),
    ("hubot", "Preview deployed to <code>preview.example.com</code>."),
    ("sam-codes", "Could we add a test for the empty case before merging?"),
    ("octo-dev", "Good catch, added in the latest commit."),
]


def comment_events(item, kind):
    count = min(item["comments"], len(COMMENTS))
    nodes = []
    for index in range(count):
        login, text = COMMENTS[index]
        nodes.append({
            "author": actor(login),
            "createdAt": ago(hours=(count - index) * 5),
            "bodyHTML": f"<p>{text}</p>",
            "url": f"{item_url(item, kind)}#issuecomment-{90000 + index}",
        })
    return {"nodes": nodes}


REVIEW_TEXT = {
    "APPROVED": "Looks good to me.",
    "CHANGES_REQUESTED": "A couple of things to address before this can go in; see inline notes.",
    "COMMENTED": "Left a few small suggestions.",
}


def detail(item, kind):
    node = common(item, kind)
    node.update({
        "bodyHTML": body_html(item, kind),
        "viewerDidAuthor": item["author"] == VIEWER,
        "viewerCanReact": True,
        "locked": False,
        "commentEvents": comment_events(item, kind),
    })
    if kind != "PR":
        return node
    submitted = [(login, state) for login, state in item["reviewers"] if state != "PENDING"]
    pending = [login for login, state in item["reviewers"] if state == "PENDING"]
    reviews = [{
        "author": actor(login),
        "state": state,
        "createdAt": ago(hours=3 + index),
        "submittedAt": ago(hours=3 + index),
        "bodyHTML": f"<p>{REVIEW_TEXT[state]}</p>",
        "body": REVIEW_TEXT[state],
        "url": f"{item_url(item, kind)}#pullrequestreview-{50000 + index}",
    } for index, (login, state) in enumerate(submitted)]
    latest = [{"author": actor(login), "state": state} for login, state in submitted]
    node.update({
        "isDraft": item["draft"],
        "additions": item["additions"],
        "deletions": item["deletions"],
        "headRefName": item["head"],
        "baseRefName": "main",
        "reviewDecision": item["review"],
        "changedFiles": len(diff_files(item)),
        "latestReviews": {"nodes": latest},
        "latestOpinionatedReviews": {"nodes": [r for r in latest if r["state"] != "COMMENTED"]},
        "reviewRequests": {"nodes": [
            {"requestedReviewer": {"__typename": "User", **actor(login)}} for login in pending
        ] + ([{"requestedReviewer": {"__typename": "Team", "combinedSlug": "acme/frontend",
                                     "name": "Frontend"}}] if item["repo"] == "acme/web" else [])},
        "commits": {"nodes": [{"commit": {"statusCheckRollup": {
            "state": item["checks"], "contexts": {"nodes": check_contexts(item)}}}}]},
        "reviews": {"nodes": reviews},
    })
    return node


def diff_files(pr):
    stem = pr["head"].split("/")[-1]
    return [
        {"filename": f"src/{stem}/index.ts", "status": "modified", "additions": 24, "deletions": 6,
         "patch": ("@@ -1,8 +1,12 @@\n import { createStore } from '../store';\n"
                   "-import { legacyHandler } from './legacy';\n+import { handler } from './handler';\n"
                   "+import { track } from '../telemetry';\n \n export function setup() {\n"
                   "-  return legacyHandler();\n+  const result = handler();\n+  track('setup');\n"
                   "+  return result;\n }\n")},
        {"filename": f"src/{stem}/handler.ts", "status": "added", "additions": 18, "deletions": 0,
         "patch": ("@@ -0,0 +1,6 @@\n+export function handler() {\n+  const started = Date.now();\n"
                   "+  // Keep the work small and observable.\n+  return { started, ok: true };\n+}\n+\n")},
        {"filename": f"src/{stem}/legacy.ts", "status": "removed", "additions": 0, "deletions": 9,
         "patch": ("@@ -1,4 +0,0 @@\n-export function legacyHandler() {\n-  return { ok: false };\n"
                   "-}\n-\n")},
        {"filename": f"test/{stem}.test.ts", "status": "modified", "additions": 11, "deletions": 2,
         "patch": ("@@ -3,6 +3,9 @@ describe('setup', () => {\n   it('returns a result', () => {\n"
                   "-    expect(setup()).toBeDefined();\n+    const result = setup();\n"
                   "+    expect(result.ok).toBe(true);\n+    expect(result.started).toBeGreaterThan(0);\n"
                   "   });\n });\n")},
    ]


# ---- Agent tasks ---------------------------------------------------------------

TASKS = [
    dict(id="task-3f0a91", name="Add loading skeletons to the billing page", repo="acme/web",
         state="in_progress", created=dict(minutes=40), updated=dict(minutes=3), pr=488,
         pr_state="OPEN", draft=True, model="claude-sonnet-4.6"),
    dict(id="task-7c21de", name="Investigate flaky webhook delivery test", repo="acme/api",
         state="waiting_for_user", created=dict(hours=2), updated=dict(minutes=18), pr=1188,
         pr_state="OPEN", draft=True, model="gpt-5.4"),
    dict(id="task-b4e602", name="Write migration guide for Toolbar v2", repo="acme/design-system",
         state="completed", created=dict(days=1, hours=2), updated=dict(hours=20), pr=99,
         pr_state="OPEN", draft=False, model="claude-opus-4.6"),
    dict(id="task-19d3c7", name="Upgrade the API to the new logging library", repo="acme/api",
         state="failed", created=dict(days=2), updated=dict(days=1, hours=22), pr=None,
         pr_state=None, draft=False, model="gpt-5.3-codex"),
    dict(id="task-e85a10", name="Fix typo in onboarding email template", repo="acme/web",
         state="queued", created=dict(minutes=2), updated=dict(minutes=2), pr=None,
         pr_state=None, draft=False, model="claude-sonnet-4.6"),
    dict(id="task-a0c4f8", name="Remove unused feature flags", repo="acme/web",
         state="completed", created=dict(days=4), updated=dict(days=3, hours=20), pr=476,
         pr_state="MERGED", draft=False, model="gpt-5.4"),
]


PROMPTS = {
    "task-3f0a91": "The billing page is blank while invoices load. Add loading skeletons that "
                   "match the invoice table and keep the summary card visible.",
    "task-7c21de": "test_webhook_delivery_retries fails about one run in ten on CI. Find the "
                   "cause and make the test deterministic without lowering coverage.",
    "task-b4e602": "Write a migration guide from Toolbar v1 to v2 with before/after examples "
                   "for every removed prop.",
}


def prompt(task):
    return PROMPTS.get(task["id"], task["name"])


def pull_database_id(task):
    return 880000 + task["pr"] if task["pr"] else None


def task_json(task, detail=False):
    raw = {
        "id": task["id"],
        "name": task["name"],
        "state": task["state"],
        "html_url": f"https://github.com/{task['repo']}/agents/tasks/{task['id']}",
        "created_at": ago(**task["created"]),
        "updated_at": ago(**task["updated"]),
        "repository": {"id": REPOS[task["repo"]], "full_name": task["repo"]},
        "artifacts": [],
    }
    if task["pr"]:
        raw["artifacts"] = [{"provider": "github", "type": "pull", "data": {
            "id": pull_database_id(task), "global_id": f"PR_task_{task['pr']}"}}]
    if detail:
        raw["sessions"] = sessions(task)
    return raw


def sessions(task):
    first_failed = task["state"] in ("completed", "in_progress") and task["pr"] is not None
    result = []
    if first_failed and task["id"] == "task-b4e602":
        result.append({
            "id": f"{task['id']}-s1", "name": "First attempt", "state": "failed",
            "created_at": ago(**task["created"]), "updated_at": ago(**task["created"]),
            "completed_at": ago(**task["created"]), "prompt": prompt(task),
            "head_ref": f"copilot/{task['id']}", "base_ref": "main", "model": task["model"],
            "usage": {"type": "premium_requests", "amount": 1.0},
            "error": {"message": "The setup step timed out installing dependencies."},
        })
    done = task["state"] in ("completed", "failed")
    session = {
        "id": f"{task['id']}-s{len(result) + 1}", "name": task["name"], "state": task["state"],
        "created_at": ago(**task["created"]), "updated_at": ago(**task["updated"]),
        "completed_at": ago(**task["updated"]) if done else None, "prompt": prompt(task),
        "head_ref": f"copilot/{task['id']}", "base_ref": "main", "model": task["model"],
        "usage": {"type": "premium_requests", "amount": 1.0 if done else 0.5},
    }
    if task["state"] == "failed":
        session["error"] = {"message": "Tests failed after 3 attempts: logger.spec.ts"}
    result.append(session)
    return result


def find_task(task_id):
    for task in TASKS:
        if task_id.startswith(task["id"]):
            return task
    return None


def session_log(task):
    created = int((NOW - timedelta(**task["created"])).timestamp())
    lines = [
        {"role": "user", "content": prompt(task)},
        {"object": "chat.completion.chunk", "created": created + 5, "choices": [{
            "finish_reason": None, "delta": {"role": "assistant",
                                             "content": "I'll start by looking at the relevant files."}}]},
        {"object": "chat.completion.chunk", "created": created + 9, "choices": [{
            "finish_reason": None, "delta": {"role": "assistant", "content": "", "tool_calls": [{
                "id": "call_1", "type": "function", "function": {
                    "name": "bash", "arguments": json.dumps({"command": "rg -l TODO src | head"})}}]}}]},
        {"role": "tool", "tool_call_id": "call_1",
         "content": "src/billing/page.tsx\nsrc/billing/summary.tsx\n", "toolTelemetry": {"durationMs": 41}},
        {"object": "chat.completion.chunk", "created": created + 20, "choices": [{
            "finish_reason": None, "delta": {"role": "assistant", "content": "", "tool_calls": [{
                "id": "call_2", "type": "function", "function": {
                    "name": "str_replace_editor", "arguments": json.dumps(
                        {"command": "view", "path": "src/billing/page.tsx"})}}]}}]},
        {"role": "tool", "tool_call_id": "call_2",
         "content": "export function BillingPage() {\n  const data = useInvoices();\n  ...\n}",
         "toolTelemetry": {"durationMs": 7}},
        {"object": "chat.completion.chunk", "created": created + 31, "choices": [{
            "finish_reason": None, "delta": {"role": "assistant", "content": (
                "The page renders nothing while invoices load. I'll add a skeleton that matches "
                "the table layout and keep the summary card visible.")}}]},
        {"object": "chat.completion.chunk", "created": created + 32, "choices": [{
            "finish_reason": "stop", "delta": {}}]},
    ]
    return "".join(f"data: {json.dumps(line)}\n\n" for line in lines) + "data: [DONE]\n\n"


# ---- Actions -------------------------------------------------------------------

WORKFLOWS = [
    (101, "CI", ".github/workflows/ci.yml"),
    (102, "Deploy", ".github/workflows/deploy.yml"),
    (103, "Nightly", ".github/workflows/nightly.yml"),
]

RUNS = [
    # repo, id, workflow, status, conclusion, branch, event, actor, message, age
    ("acme/web", 9001, 101, "in_progress", None, "palette-shortcuts", "pull_request", "octo-dev",
     "Add shortcut hints to palette rows", dict(minutes=4)),
    ("acme/web", 9002, 102, "waiting", None, "main", "push", "mona",
     "Release 2.14.0", dict(minutes=35)),
    ("acme/web", 9003, 101, "completed", "success", "main", "push", "mona",
     "Merge pull request #476 from acme/remove-flags", dict(hours=3)),
    ("acme/web", 9004, 101, "completed", "failure", "settings-forms", "pull_request", "lin-dev",
     "Use FormField in the profile section", dict(hours=5)),
    ("acme/api", 9101, 101, "completed", "failure", "audit-log-pages", "pull_request", "octo-dev",
     "Add cursor pagination to the audit log", dict(hours=2)),
    ("acme/api", 9102, 103, "completed", "success", "main", "schedule", "hubot",
     "Nightly build", dict(hours=9)),
    ("acme/api", 9103, 102, "completed", "cancelled", "main", "workflow_dispatch", "ada-ops",
     "Deploy to staging", dict(days=1)),
    ("acme/design-system", 9201, 101, "queued", None, "chart-dark-tokens", "pull_request",
     "octo-dev", "Dark chart palette", dict(minutes=1)),
    ("acme/design-system", 9202, 101, "completed", "success", "main", "push", "mona",
     "Publish 5.3.0", dict(days=2)),
]


def workflow(workflow_id):
    return next(w for w in WORKFLOWS if w[0] == workflow_id)


def actions_actor(login):
    return {"login": login, "avatar_url": "", "html_url": f"https://github.com/{login}"}


def run_json(run):
    repo, run_id, workflow_id, status, conclusion, branch, event, login, message, age = run
    _, name, path = workflow(workflow_id)
    return {
        "id": run_id, "name": name, "path": path, "workflow_id": workflow_id,
        "run_number": run_id % 1000 + 200, "run_attempt": 1, "event": event, "status": status,
        "conclusion": conclusion, "head_branch": branch, "head_sha": sha(f"{repo}{run_id}"),
        "html_url": f"https://github.com/{repo}/actions/runs/{run_id}",
        "created_at": ago(**age), "updated_at": ago(**age), "run_started_at": ago(**age),
        "actor": actions_actor(login), "triggering_actor": actions_actor(login),
        "head_commit": {"id": sha(f"{repo}{run_id}"), "message": message, "timestamp": ago(**age)},
    }


def jobs_json(run):
    repo, run_id, workflow_id, status, conclusion, *_rest = run
    start = NOW - timedelta(**run[-1])

    def at(minutes):
        return (start + timedelta(minutes=minutes)).strftime("%Y-%m-%dT%H:%M:%SZ")

    def job(index, name, job_status, job_conclusion, steps):
        return {
            "id": run_id * 10 + index, "run_id": run_id, "name": name, "status": job_status,
            "conclusion": job_conclusion,
            "html_url": f"https://github.com/{repo}/actions/runs/{run_id}/job/{run_id * 10 + index}",
            "started_at": at(index), "completed_at": at(index + 3) if job_status == "completed" else None,
            "runner_name": f"acme-runner-{index}", "labels": ["ubuntu-latest"],
            "steps": [{"number": n + 1, "name": step, "status": job_status,
                       "conclusion": job_conclusion, "started_at": at(index),
                       "completed_at": at(index + 1) if job_status == "completed" else None}
                      for n, step in enumerate(steps)],
        }

    done = status == "completed"
    test_conclusion = conclusion if done else None
    if workflow_id == 102:
        return [
            job(1, "build", "completed", "success", ["Checkout", "Build image", "Push image"]),
            job(2, "deploy-staging", "completed" if done else "completed",
                "success" if status != "completed" or conclusion == "success" else conclusion,
                ["Deploy", "Smoke test"]),
            job(3, "deploy-production", status if done else "waiting", test_conclusion,
                ["Deploy", "Smoke test"]),
        ]
    return [
        job(1, "lint", "completed", "success", ["Checkout", "Install", "Lint"]),
        job(2, "build", "completed" if done else "in_progress",
            "success" if done else None, ["Checkout", "Install", "Build"]),
        job(3, "test", status, test_conclusion, ["Checkout", "Install", "Test"]),
        job(4, "e2e", status, test_conclusion if conclusion != "failure" else "skipped",
            ["Checkout", "Install", "Playwright"]),
    ]


WORKFLOW_FILES = {
    101: "name: CI\non: [push, pull_request]\njobs:\n  lint:\n    runs-on: ubuntu-latest\n"
         "    steps: [{run: npm run lint}]\n  build:\n    runs-on: ubuntu-latest\n"
         "    steps: [{run: npm run build}]\n  test:\n    needs: [lint, build]\n"
         "    runs-on: ubuntu-latest\n    steps: [{run: npm test}]\n  e2e:\n    needs: [test]\n"
         "    runs-on: ubuntu-latest\n    steps: [{run: npm run e2e}]\n",
    102: "name: Deploy\non:\n  push: {branches: [main]}\n  workflow_dispatch:\n    inputs:\n"
         "      environment:\n        type: environment\n        required: true\n"
         "jobs:\n  build:\n    runs-on: ubuntu-latest\n    steps: [{run: make image}]\n"
         "  deploy-staging:\n    needs: build\n    environment: staging\n"
         "    runs-on: ubuntu-latest\n    steps: [{run: make deploy}]\n  deploy-production:\n"
         "    needs: deploy-staging\n    environment: production\n    runs-on: ubuntu-latest\n"
         "    steps: [{run: make deploy}]\n",
    103: "name: Nightly\non:\n  schedule: [{cron: '0 3 * * *'}]\njobs:\n  lint:\n"
         "    runs-on: ubuntu-latest\n    steps: [{run: npm run lint}]\n  build:\n"
         "    runs-on: ubuntu-latest\n    steps: [{run: npm run build}]\n  test:\n"
         "    needs: [build]\n    runs-on: ubuntu-latest\n    steps: [{run: npm test}]\n  e2e:\n"
         "    needs: [test]\n    runs-on: ubuntu-latest\n    steps: [{run: npm run e2e}]\n",
}

ENVIRONMENTS = [
    (301, "staging", []),
    (302, "production", [{"id": 1, "type": "required_reviewers", "reviewers": [
        {"type": "User", "reviewer": {"login": "octo-dev"}},
        {"type": "Team", "reviewer": {"slug": "release-managers", "name": "Release managers"}}]}]),
]


def environment_json(repo, environment):
    env_id, name, rules = environment
    return {"id": env_id, "name": name,
            "url": f"{BASE_URL}/repos/{repo}/environments/{name}",
            "html_url": f"https://github.com/{repo}/deployments/{name}",
            "deployment_branch_policy": {"protected_branches": True, "custom_branch_policies": False},
            "protection_rules": rules}


def deployments_json(repo, environment):
    index = 1 if environment == "staging" else 2
    return [{
        "id": 7000 + REPOS[repo] % 100 * 10 + index, "ref": "main",
        "sha": sha(f"{repo}{environment}"), "environment": environment,
        "description": f"Deploy {repo.split('/')[1]} to {environment}",
        "created_at": ago(hours=index * 6), "updated_at": ago(hours=index * 6),
        "task": "deploy", "payload": {},
    }]


RELEASES = {
    "acme/web": [("v2.14.0", "2.14.0", False, dict(minutes=40)), ("v2.13.2", "2.13.2", False, dict(days=6))],
    "acme/api": [("v1.9.0", "1.9.0", False, dict(days=3)), ("v1.10.0-rc.1", "1.10.0 RC 1", True, dict(hours=20))],
    "acme/design-system": [("v5.3.0", "5.3.0", False, dict(days=2))],
}


def releases_json(repo):
    return [{
        "id": 60000 + index + REPOS[repo], "tag_name": tag, "name": name,
        "body": "## Changes\n- Faster startup\n- Fixes for keyboard navigation\n",
        "draft": False, "prerelease": pre,
        "html_url": f"https://github.com/{repo}/releases/tag/{tag}",
        "created_at": ago(**age), "published_at": ago(**age), "author": actions_actor("mona"),
    } for index, (tag, name, pre, age) in enumerate(RELEASES.get(repo, []))]


def tags_json(repo):
    tags = [{"name": tag, "commit": {"sha": sha(f"{repo}{tag}"), "url": ""}}
            for tag, *_ in RELEASES.get(repo, [])]
    if repo == "acme/api":
        tags.append({"name": "v1.8.9-hotfix", "commit": {"sha": sha("hotfix"), "url": ""}})
    return tags


# ---- HTTP ------------------------------------------------------------------------


# ---- Sign in with GitHub -----------------------------------------------------

SIGN_IN = "mona"
USER_CODE = "OCTO-1234"
DEVICE_CODE = "fixture-device-code"
DEVICE_INTERVAL = 2
PENDING_POLLS = 2
DEVICE_POLLS = {}


# Loopback hosts a callback may name, as they are written in a URL.
LOOPBACK_HOSTS = {"127.0.0.1": "127.0.0.1", "::1": "[::1]"}


def loopback_redirect(uri):
    """A plain `http` loopback callback with a valid port, and no whitespace or
    control characters, which could split the Location header."""
    uri = uri or ""
    if any(ord(char) <= 0x20 or ord(char) == 0x7F for char in uri):
        return False
    parts = urlparse(uri)
    try:
        parts.port
    except ValueError:
        return False
    return parts.scheme == "http" and parts.hostname in LOOPBACK_HOSTS


def authorize(query):
    """The Location to send the browser to, or None for a refused request. It
    is rebuilt from the checked host, port and escaped path, never copied."""
    redirect = query.get("redirect_uri", [""])[0]
    if not loopback_redirect(redirect):
        return None
    parts = urlparse(redirect)
    host = LOOPBACK_HOSTS[parts.hostname]
    port = f":{parts.port}" if parts.port else ""
    path = quote(parts.path or "/", safe="/")
    pairs = {"code": f"fixture-code-{SIGN_IN}", "state": query.get("state", [""])[0]}
    return f"http://{host}{port}{path}?{urlencode(pairs)}"


def granted():
    return {"access_token": f"fixture-{SIGN_IN}", "token_type": "bearer", "scope": "repo,read:org"}


def access_token(form):
    if form.get("grant_type", [""])[0] == "urn:ietf:params:oauth:grant-type:device_code":
        code = form.get("device_code", [""])[0]
        if code != DEVICE_CODE:
            return {"error": "bad_verification_code", "error_description": "The device code is not the fixture's."}
        DEVICE_POLLS[code] = DEVICE_POLLS.get(code, 0) + 1
        if DEVICE_POLLS[code] <= PENDING_POLLS:
            return {"error": "authorization_pending"}
        DEVICE_POLLS.pop(code)
        return granted()
    if form.get("code", [""])[0] != f"fixture-code-{SIGN_IN}" or not form.get("code_verifier", [""])[0]:
        return {"error": "bad_verification_code", "error_description": "The code is not the fixture's."}
    return granted()


def device_code():
    return {"device_code": DEVICE_CODE, "user_code": USER_CODE,
            "verification_uri": f"{BASE_URL}/login/device", "expires_in": 900, "interval": DEVICE_INTERVAL}


def log(message):
    print(message, file=sys.stderr, flush=True)


def graphql(body, login):
    query = body.get("query", "")
    variables = body.get("variables") or {}
    if re.search(r"\bviewer\s*\{", query):
        return {"viewer": {"id": f"U_{login}", "login": login,
                           "copilotEndpoints": {"api": "https://copilot-api.example.com"}}}
    if "search(" in query:
        return search(query, variables)
    if "nodes(ids:" in query:
        nodes = []
        for node in variables.get("ids", []):
            number = int(node.rsplit("_", 1)[-1]) if node.startswith("PR_task_") else None
            task = next((t for t in TASKS if t["pr"] == number), None)
            nodes.append(None if task is None else {
                "databaseId": pull_database_id(task), "number": task["pr"],
                "url": f"https://github.com/{task['repo']}/pull/{task['pr']}",
                "state": task["pr_state"], "isDraft": task["draft"]})
        return {"nodes": nodes}
    if "assignableUsers" in query:
        return {"repository": {"assignableUsers": {"nodes": [
            actor(name) for name in PEOPLE if name != WORK_VIEWER]}}}
    if "issueOrPullRequest" in query:
        item, kind = find_item(variables.get("owner", ""), variables.get("name", ""),
                               int(variables.get("number", 0)))
        if item is None:
            log(f"UNKNOWN item {variables}")
            return {"repository": {"issueOrPullRequest": None}}
        if "bodyHTML" in query:
            data = {"repository": {"issueOrPullRequest": detail(item, kind)}}
        else:
            data = {"repository": {"issueOrPullRequest": {
                "__typename": "PullRequest" if kind == "PR" else "Issue", "id": node_id(kind, item)}}}
        for key, value in variables.items():
            if re.fullmatch(r"l\d+", key):
                data[f"u{key[1:]}"] = {"id": f"U_{value}", "login": value}
        return data
    if query.lstrip().startswith("mutation"):
        return {"ok": {"clientMutationId": None}}
    log(f"UNKNOWN GraphQL: {query[:160]!r}")
    return {}


SEARCHABLE_REPOS = [
    ("acme/web", "Customer web app", True),
    ("acme/api", "Public REST and webhook API", True),
    ("acme/design-system", "Components and tokens", False),
    ("acme/mobile", "iOS and Android apps", True),
    ("acme/docs", "Product documentation", False),
    ("acme/infra", "Terraform and deployment", True),
    ("octo-dev/dotfiles", "Shell and editor setup", False),
    ("mona/octo-tools", "Small command-line helpers", False),
]


def repo_json(name, description, private):
    return {"full_name": name, "description": description, "private": private, "archived": False}


def rest(path, query, login):
    parts = [part for part in path.split("/") if part]
    if parts == ["user", "repos"]:
        return 200, [repo_json(*repo) for repo in SEARCHABLE_REPOS[:6]]
    if parts == ["search", "repositories"]:
        text = query.get("q", [""])[0].lower()
        found = [repo_json(*repo) for repo in SEARCHABLE_REPOS
                 if any(word in (repo[0] + " " + repo[1]).lower() for word in text.split())]
        return 200, {"total_count": len(found), "incomplete_results": False, "items": found}
    if parts == ["user"]:
        return 200, {"login": login, "id": 1, "name": PEOPLE.get(login)}
    if parts[:2] == ["agents", "tasks"]:
        if len(parts) == 2:
            return 200, {"tasks": [task_json(task) for task in TASKS]}
        task = find_task(parts[2])
        return (200, task_json(task, detail=True)) if task else (404, {"message": "Not Found"})
    if parts[:2] == ["agents", "repos"] and len(parts) >= 6:
        task = find_task(parts[5])
        return (200, task_json(task, detail=True)) if task else (404, {"message": "Not Found"})
    if parts[:2] == ["agents", "sessions"] and len(parts) >= 3:
        task = find_task(parts[2])
        if task is None:
            return 404, {"message": "Not Found"}
        if len(parts) == 4 and parts[3] == "logs":
            return 200, session_log(task)
        return 200, {"id": parts[2], "name": task["name"], "state": task["state"],
                     "model": task["model"], "task_id": task["id"],
                     "head_ref": f"copilot/{task['id']}", "base_ref": "main",
                     "resource_type": "pull" if task["pr"] else "", "resource_number": task["pr"],
                     "created_at": ago(**task["created"]), "last_updated_at": ago(**task["updated"])}
    if parts[:1] == ["repositories"] and len(parts) == 2:
        repo = next((name for name, rid in REPOS.items() if str(rid) == parts[1]), None)
        return (200, {"id": int(parts[1]), "full_name": repo}) if repo else (404, {"message": "Not Found"})
    if parts[:1] != ["repos"] or len(parts) < 3:
        return None
    repo = f"{parts[1]}/{parts[2]}".lower()
    rest_parts = parts[3:]
    if not rest_parts:
        return 200, {"full_name": repo, "default_branch": "main", "id": REPOS.get(repo, 0)}
    head = rest_parts[0]
    if head == "pulls" and len(rest_parts) == 1:
        return 200, [{"id": pull_database_id(t), "number": t["pr"], "state": "open" if t["pr_state"] == "OPEN" else "closed",
                      "draft": t["draft"], "merged_at": ago(days=3) if t["pr_state"] == "MERGED" else None,
                      "html_url": f"https://github.com/{t['repo']}/pull/{t['pr']}"}
                     for t in TASKS if t["repo"] == repo and t["pr"]]
    if head == "pulls" and len(rest_parts) >= 2:
        pr, _ = find_item(parts[1], parts[2], int(rest_parts[1]))
        if pr is None or "head" not in pr:
            return 404, {"message": "Not Found"}
        if len(rest_parts) == 3 and rest_parts[2] == "files":
            page = query.get("page", ["1"])[0]
            return 200, diff_files(pr) if page == "1" else []
        return 200, {"head": {"sha": sha(pr["head"])}, "base": {"sha": sha("main" + repo)},
                     "changed_files": len(diff_files(pr))}
    if head == "compare":
        base = rest_parts[1].split("...")[0] if len(rest_parts) > 1 else ""
        return 200, {"merge_base_commit": {"sha": base}}
    if head == "contents":
        file_path = "/".join(rest_parts[1:])
        for workflow_id, _, path in WORKFLOWS:
            if file_path == path:
                return 200, WORKFLOW_FILES[workflow_id]
        return 200, f"// {file_path}\nexport const value = 42;\n"
    if head == "actions":
        return actions(repo, rest_parts[1:])
    if head == "environments":
        if len(rest_parts) == 1:
            return 200, {"total_count": len(ENVIRONMENTS),
                         "environments": [environment_json(repo, env) for env in ENVIRONMENTS]}
        return None
    if head == "deployments":
        if len(rest_parts) == 1:
            environment = query.get("environment", ["staging"])[0]
            return 200, deployments_json(repo, environment)
        if len(rest_parts) == 3 and rest_parts[2] == "statuses":
            deployment_id = int(rest_parts[1])
            state = "success" if deployment_id % 10 == 1 else "in_progress"
            return 200, [{"id": deployment_id * 10, "state": state, "description": "Deployed",
                          "environment_url": f"https://{repo.split('/')[1]}.example.com",
                          "created_at": ago(hours=1), "updated_at": ago(hours=1)}]
        return None
    if head == "releases" and len(rest_parts) == 1:
        return 200, releases_json(repo)
    if head == "tags" and len(rest_parts) == 1:
        return 200, tags_json(repo)
    return None


def actions(repo, parts):
    runs = [run for run in RUNS if run[0] == repo]
    if parts == ["workflows"]:
        return 200, {"total_count": len(WORKFLOWS), "workflows": [
            {"id": wid, "name": name, "path": path, "state": "active",
             "html_url": f"https://github.com/{repo}/actions/workflows/{path.rsplit('/', 1)[-1]}"}
            for wid, name, path in WORKFLOWS]}
    if parts == ["runs"]:
        time.sleep(RUNS_DELAY)
        return 200, {"total_count": len(runs), "workflow_runs": [run_json(run) for run in runs]}
    if len(parts) == 3 and parts[0] == "workflows" and parts[2] == "runs":
        chosen = [run for run in runs if str(run[2]) == parts[1] or workflow(run[2])[2].endswith(parts[1])]
        return 200, {"total_count": len(chosen), "workflow_runs": [run_json(run) for run in chosen]}
    if len(parts) >= 2 and parts[0] == "runs":
        run = next((run for run in runs if str(run[1]) == parts[1]), None)
        if run is None:
            return 404, {"message": "Not Found"}
        if len(parts) == 2:
            return 200, run_json(run)
        if parts[2] == "jobs":
            jobs = jobs_json(run)
            return 200, {"total_count": len(jobs), "jobs": jobs}
        if parts[2] == "pending_deployments":
            if run[3] != "waiting":
                return 200, []
            return 200, [{"environment": environment_json(repo, ENVIRONMENTS[1]), "wait_timer": 0,
                          "wait_timer_started_at": ago(minutes=30),
                          "reviewers": ENVIRONMENTS[1][2][0]["reviewers"],
                          "current_user_can_approve": True}]
        return None
    if len(parts) == 3 and parts[0] == "jobs" and parts[2] == "logs":
        return 200, job_log(parts[1])
    return None


def job_log(job_id):
    lines = [
        "##[group]Run actions/checkout@v4", "Syncing repository: acme/web", "##[endgroup]",
        "##[group]Run npm ci", "added 1184 packages in 21s", "##[endgroup]",
        "##[group]Run npm test", "> vitest run", "",
        " ✓ src/palette/index.test.ts (12 tests) 84ms",
        " ✓ src/settings/form.test.ts (31 tests) 212ms",
        " × src/audit/pagination.test.ts > returns the next cursor",
        "   AssertionError: expected undefined to be 'c_20'", "",
        " Test Files  1 failed | 2 passed (3)", "      Tests  1 failed | 43 passed (44)",
        "##[error]Process completed with exit code 1.", "##[endgroup]",
    ]
    stamp = NOW - timedelta(minutes=20)
    return "\n".join(f"{(stamp + timedelta(seconds=i * 2)).strftime('%Y-%m-%dT%H:%M:%S.0000000Z')} {line}"
                     for i, line in enumerate(lines)) + "\n"


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        pass

    def log_request_line(self, body=b""):
        detail = ""
        if body:
            try:
                query = json.loads(body).get("query", "")
                detail = " " + " ".join(query.split())[:70]
            except ValueError:
                pass
        log(f"{self.command} {self.path} as {self.login()}{detail}")

    def login(self):
        header = self.headers.get("Authorization", "")
        token = header.split(" ", 1)[-1].strip()
        candidate = token[len("fixture-"):] if token.startswith("fixture-") else ""
        return candidate if candidate in PEOPLE else VIEWER

    def reply(self, status, payload):
        if isinstance(payload, str):
            data = payload.encode()
            content_type = "text/plain; charset=utf-8"
        else:
            data = json.dumps(payload).encode()
            content_type = "application/json; charset=utf-8"
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def read_body(self):
        length = int(self.headers.get("Content-Length") or 0)
        return self.rfile.read(length) if length else b""

    def redirect(self, location):
        # Whatever built it, a line break can never split the header.
        safe_location = location.replace("\r", "").replace("\n", "")
        self.send_response(302)
        self.send_header("Location", safe_location)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_DELETE(self):
        url = urlparse(self.path)
        self.read_body()
        log(f"DELETE {url.path}")
        parts = [part for part in url.path.split("/") if part]
        if len(parts) == 3 and parts[0] == "applications" and parts[2] == "grant":
            self.send_response(204)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        self.reply(404, {"message": "Not Found"})

    def do_POST(self):
        body = self.read_body()
        url = urlparse(self.path)
        if url.path.startswith("/login/"):
            # The form carries codes and the client secret: log the path only.
            log(f"POST {url.path}")
            form = parse_qs(body.decode(errors="replace"))
            if url.path == "/login/oauth/access_token":
                self.reply(200, access_token(form))
            elif url.path == "/login/device/code":
                self.reply(200, device_code())
            else:
                self.reply(404, {"message": "Not Found"})
            return
        self.log_request_line(body)
        try:
            if url.path.rstrip("/") == "/graphql":
                payload = json.loads(body or b"{}")
                self.reply(200, {"data": graphql(payload, self.login())})
                return
            log(f"POST {self.path} (accepted without effect)")
            self.reply(200 if "/agents/" not in url.path else 201,
                       {"id": "task-new", "state": "queued"} if "/agents/" in url.path else {})
        except Exception as error:  # noqa: BLE001 - keep serving
            log(f"error in POST {self.path}: {error!r}")
            self.reply(200, {"data": {}})

    def do_GET(self):
        url = urlparse(self.path)
        if url.path == "/login/oauth/authorize":
            log(f"GET {url.path}")
            location = authorize(parse_qs(url.query))
            if location is None:
                self.reply(400, {"message": "redirect_uri must be a loopback address"})
            else:
                self.redirect(location)
            return
        self.log_request_line()
        try:
            answer = rest(url.path, parse_qs(url.query), self.login())
        except Exception as error:  # noqa: BLE001 - keep serving
            log(f"error in GET {self.path}: {error!r}")
            answer = None
        if answer is None:
            log(f"UNKNOWN GET {self.path}")
            answer = (200, [] if url.path.endswith("s") else {})
        self.reply(*answer)


def main():
    global BASE_URL, RUNS_DELAY
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--port", type=int, default=18162)
    parser.add_argument("--runs-delay", type=float, default=0.0,
                        help="seconds to hold each Actions run list")
    args = parser.parse_args()
    RUNS_DELAY = args.runs_delay
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    BASE_URL = f"http://127.0.0.1:{server.server_port}"
    print(f"SUBSPACE_GITHUB_API_URL={BASE_URL}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
