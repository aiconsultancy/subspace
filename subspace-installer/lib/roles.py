"""Render fleet roles and default OpenCode permissions."""

from pathlib import Path
from string import Template

from agent_files import agent_directory, write_private


ROLES = Path(__file__).parent / "roles"
SKILLS = ("fleet-rear-admiral", "fleet-captain", "fleet-crew")


def default_permissions(role, references):
    rules = [{"action": "*", "resource": "*", "effect": "deny"}]
    rules.extend({"action": action, "resource": "*", "effect": "allow"} for action in ("read", "glob", "grep", "question", "skill"))
    if references and role == "rear_admiral":
        rules.append({"action": "external_directory", "resource": f"{references}/*", "effect": "allow"})
    for resource in ("*.env", "*.env.*", "*.pem", "*.key"):
        rules.append({"action": "read", "resource": resource, "effect": "deny"})
    rules.append({"action": "read", "resource": "*.env.example", "effect": "allow"})
    commands = ["git status", "git diff", "git log", "git show"]
    if role != "rear_admiral":
        rules.append({"action": "edit", "resource": "*", "effect": "allow"})
        commands += ["git add", "git commit", "git fetch", "make", "npm test", "npm run", "bun test", "bun run",
                     "cargo check", "cargo test", "cargo build", "go test", "python3 -m unittest", "pytest"]
    rules.extend({"action": "shell", "resource": f"{command} *", "effect": "allow"} for command in commands)
    return rules


def render(role, name, values):
    return Template((ROLES / role / name).read_text()).substitute(values)


def role_documents(cfg, person, references, home):
    """Instructions and skills for an officer, plus a system prompt per crew member it hosts."""
    role = person["role"].replace("_", "-")
    repositories = list(cfg["repositories"])
    values = {
        "agent": person["name"], "fleet": cfg["name"], "channel": person["channel"],
        "references": str(references) if references else "your workspace when repositories are configured",
        "repositories": ", ".join(f"`{name}`" for name in repositories),
        "first": repositories[0] if repositories else "",
        "worktrees": f"{home}/worktrees",
    }
    if person.get("crew"):
        roster = "\n".join(f"- {worker['name']} (`{worker['id']}`, {worker['family'].title()} family, "
                           f"model `{worker['model']}`)" for worker in person["crew"])
        values["crew"] = render("captain", "CREW.md", {**values, "roster": roster}).rstrip("\n")
    else:
        values["crew"] = "You have no crew in this fleet. Do the work yourself in your checkout."
    instructions = render(role, "AGENTS.md", values)
    skills = {f"fleet-{role}": render(role, "SKILL.md", values)}
    crew = {}
    if person.get("crew"):
        skills["fleet-crew"] = render("crew", "SKILL.md", values)
        for worker in person["crew"]:
            worktrees = "\n".join(f"- `{name}`: {home}/worktrees/{worker['id']}/{name}" for name in repositories)
            crew[worker["id"]] = render("crew", "AGENTS.md", {
                **values, "agent": worker["name"], "id": worker["id"], "captain": person["name"],
                "model": worker["model"], "family": worker["family"].title(), "worktrees": worktrees,
            })
    procedure = skills[f"fleet-{role}"].partition("\n---\n")
    if not procedure[1]:
        raise ValueError("Fleet skill templates need Markdown front matter.")
    return {
        "role": role,
        "instructions": instructions,
        "skills": skills,
        "crew": crew,
        "claudePrompt": f"{instructions}\n{procedure[2]}",
    }


def provision_role(documents):
    """Run as the officer: write its instructions and fleet skills, removing fleet skills it no longer has."""
    if documents["role"] not in {"rear-admiral", "captain"}:
        raise ValueError("Unknown fleet role.")
    directory = agent_directory(".config", "opencode")
    write_private(directory / "AGENTS.md", documents["instructions"].encode())
    skills = documents["skills"]
    if not set(skills) <= set(SKILLS):
        raise ValueError("Unknown fleet skill.")
    for name in SKILLS:
        path = directory / "skills" / name / "SKILL.md"
        if name in skills:
            write_private(agent_directory(".config", "opencode", "skills", name) / "SKILL.md", skills[name].encode())
        elif path.is_file() and not path.is_symlink():
            path.unlink()
