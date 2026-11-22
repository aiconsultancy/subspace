"""Verify an isolated stack's fixture content through the development API."""

import json
import re
import sys
from collections import Counter
from datetime import date, timedelta
from hashlib import sha256
from pathlib import Path
from urllib.request import Request, urlopen

PORT, MODE = sys.argv[1:3]
ROOT = f"http://127.0.0.1:{int(PORT)}/api/v1"
# The channels Ergo registers, and each demo fleet's captain IDs.
CHANNELS = {row["name"] for row in json.loads(Path(__file__).with_name("irc-channels.json").read_text())}
CAPTAINS = {"aurora": ["lyra", "orion", "vega", "castor"], "borealis": ["rigel"]}


def fetch(path, *, headers=None, data=None):
    with urlopen(Request(ROOT + path, headers=headers or {}, data=data), timeout=10) as response:
        return json.load(response) if response.headers.get_content_type().endswith("json") else response.read()


token = fetch(
    "/auth/dev/token",
    headers={"Content-Type": "application/json"},
    data=json.dumps({"email": "developer@example.com", "organisation": "local"}).encode(),
)["accessToken"]


def get(path):
    return fetch(path, headers={"Authorization": f"Bearer {token}"})


def items(path):
    separator = "&" if "?" in path else "?"
    result = get(f"{path}{separator}limit=100")
    assert not result.get("nextCursor"), path
    return result["items"]


def check_empty():
    assert {fleet["slug"] for fleet in items("/fleets")} == {"aurora", "borealis", "local"}
    assert [row["name"] for row in items("/me/initiatives")] == ["Local workspace"]
    assert [row["email"] for row in items("/users")] == ["developer@example.com"]
    for path in ("/projects", "/issues", "/decisions", "/agents", "/labels"):
        assert not items(path), path
    print("Empty workspace: 3 fleets, 1 bootstrap initiative, 1 user, 0 demo issues or decisions")


def check_crews(fleets, initiatives):
    """Agent names are their IRC nicks, and initiatives list standard channels Ergo has."""
    by_slug = {fleet["slug"]: fleet for fleet in fleets}
    for slug, captains in CAPTAINS.items():
        fleet = by_slug[slug]
        assert fleet["rearAdmiralName"] == f"{slug}-admiral", fleet
        assert sorted(fleet["captainNames"]) == sorted(f"{slug}-{captain}" for captain in captains), fleet
        for agent in fleet["agents"]:
            # A captain's ship is named after it, so the desktop finds its ship
            # channel #<fleet>-<ship> in the initiative's channels.
            if agent["rank"] == "captain":
                assert agent["ship"].lower() == agent["name"].removeprefix(f"{slug}-"), agent
    for row in initiatives.values():
        listed = re.split(r"[\s,]+", row["channels"].strip())
        assert set(listed) <= CHANNELS, row
        slug = row["fleet"]["slug"]
        if slug in CAPTAINS:
            assert {f"#{slug}-starbase", f"#{slug}-command"} <= set(listed), row
            assert all(name.startswith(f"#{slug}-") for name in listed), row


def check_demo():
    fleets = items("/fleets")
    assert {fleet["slug"] for fleet in fleets} == {"aurora", "borealis", "local"}
    initiatives = {row["name"]: row for row in items("/me/initiatives")}
    assert {name: row["fleet"]["slug"] for name, row in initiatives.items()} == {
        "Local workspace": "local",
        "Navigation console": "aurora",
        "Warp diagnostics": "borealis",
        "Relay reliability": "aurora",
    }
    assert sum(len(row["stakeholders"]) for row in initiatives.values()) == 6
    check_crews(fleets, initiatives)
    navigation = get(f"/initiatives/{initiatives['Navigation console']['id']}")
    target = date.fromisoformat(navigation["targetDate"])
    week = target - timedelta(days=14)
    assert week.weekday() == 0, week
    users = items("/users")
    assert len(items("/agents")) == 7
    assert Counter(row["kind"] for row in users) == {"human": 4, "agent": 7}
    labels = items("/labels")
    assert len(labels) == 6
    projects = items("/projects")
    keys = {row["key"] for row in projects}
    assert keys == {"NAV", "WARP", "MAP", "RELAY"}
    issues = items("/issues")
    assert Counter(row["projectKey"] for row in issues) == dict.fromkeys(keys, 12)
    assert len({row["createdAt"][:10] for row in issues}) >= 10
    assert len({row["assignee"]["kind"] for row in issues if row["assignee"]}) == 2
    assert sum(row["parentId"] is not None for row in issues) == 10
    assert all(row["dueDate"] and row["estimate"] and row["labels"] for row in issues)
    for issue in issues:
        due = (date.fromisoformat(issue["dueDate"]) - week).days
        if issue["state"]["type"] == "completed":
            assert -7 <= due <= -3, issue["identifier"]
        else:
            assert 7 <= due <= 14, issue["identifier"]
    states = Counter(row["state"]["type"] for row in issues)
    assert set(states) == {"triage", "backlog", "unstarted", "started", "completed", "canceled", "duplicate"}

    expected_counts = {"comments": 22, "todos": 12, "images": 4, "attachments": 2,
                       "pull_requests": 4, "deployments": 4}
    counts = dict.fromkeys(expected_counts, 0)
    snapshots = []
    for key in sorted(keys):
        for index in (1, 2):
            path = f"/issues/{key}-{index}"
            issue = get(path)
            activity = items(f"{path}/activity")
            snapshots.append(activity)
            comments = items(f"{path}/comments")
            assert len(comments) == 1
            if key == "WARP" and index == 1:
                assert "09:46:45 UTC" in comments[0]["body"]
            replies = items(f"{path}/comments/{comments[0]['id']}/replies")
            counts["comments"] += len(comments) + len(replies)
            assert any(reply["actualAuthor"]["kind"] == "agent" for reply in replies)
            assert comments[0]["createdAt"] < replies[0]["createdAt"]
            assert len({row["displayAt"][:10] for row in activity}) >= 3
            if index == 2:
                continue
            assert "```sh" in issue["description"] and "- [x]" in issue["description"]
            assert len(issue["todos"]) == 3 and sum(row["done"] for row in issue["todos"]) == 1
            counts["todos"] += len(issue["todos"])
            snapshots.append(issue["todos"])
            images = items(f"{path}/images")
            files = items(f"{path}/attachments")
            counts["images"] += len(images)
            counts["attachments"] += len(files)
            snapshots.extend([images, files])
            assert len(images) == 1 and comments[0]["media"][0]["mediaId"] == images[0]["id"]
            assert get(images[0]["url"].removeprefix("/api/v1")).startswith(b"\x89PNG\r\n\x1a\n")
            for file in files:
                assert isinstance(get(file["url"].removeprefix("/api/v1")), bytes)
            pulls = items(f"{path}/pull-requests")
            assert len(pulls) == 1 and pulls[0]["repository"]["fullName"] == f"fallow-reach-demo/{key.lower()}"
            deployments = items(f"/projects/{key}/deployment-statuses")
            counts["pull_requests"] += len(pulls)
            counts["deployments"] += len(deployments)
            snapshots.extend([pulls, deployments])

    assert counts == expected_counts, counts
    decisions = items("/decisions")
    assert Counter(row["status"] for row in decisions) == {
        "accepted": 3, "superseded": 1, "proposed": 1, "rejected": 1,
    }
    for row in decisions:
        detail = get(f"/decisions/{row['id']}")
        assert detail["issues"] and detail["evidenceReference"].startswith("/api/v1/issues/media/")
        assert get(detail["evidenceReference"].removeprefix("/api/v1")).startswith(b"\x89PNG")
    assert get("/chat/uploads/UPL-dqKj5gLpQpub5p2m1jKAPQ/content").startswith(b"\x89PNG")
    snapshot = [fleets, initiatives, users, labels, projects, issues, decisions, snapshots]
    digest = sha256(json.dumps(snapshot, sort_keys=True).encode()).hexdigest()[:16]
    content = {
        "issues": sorted((issue["identifier"], issue["title"], issue["summary"], issue["createdAt"],
                          issue["estimate"], issue["dueDate"]) for issue in issues),
        "decisions": sorted((row["title"], row["status"], row["decidedOn"]) for row in decisions),
        "files": sorted(row["sha256"] for group in snapshots for row in group if "sha256" in row),
    }
    content_digest = sha256(json.dumps(content, sort_keys=True).encode()).hexdigest()[:16]
    print(json.dumps({"fleets": len(fleets), "initiatives": len(initiatives), "users": len(users),
                      "labels": len(labels), "projects": len(projects), "issues": len(issues),
                      "decisions": len(decisions), "states": states, **counts, "snapshot": digest, "content": content_digest},
                     sort_keys=True))


if MODE == "empty":
    check_empty()
else:
    check_demo()
