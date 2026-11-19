"""Render fleet roles and default OpenCode permissions."""

from pathlib import Path
from string import Template

from agent_files import agent_directory, write_private


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


def role_documents(cfg, person, references):
    role = person["role"].replace("_", "-")
    people = {member["id"]: member["name"] for member in cfg["members"]}
    values = {
        "agent": person["name"], "fleet": cfg["name"], "channel": person["channel"],
        "captain": people.get(person.get("captain"), ""),
        "references": str(references) if references else "your workspace when repositories are configured",
    }
    templates = Path(__file__).parent / "roles" / role
    instructions = Template((templates / "AGENTS.md").read_text()).substitute(values)
    skill = Template((templates / "SKILL.md").read_text()).substitute(values)
    _, divider, procedure = skill.partition("\n---\n")
    if not divider:
        raise ValueError("Fleet skill templates need Markdown front matter.")
    return {
        "role": role,
        "instructions": instructions,
        "skill": skill,
        "claudePrompt": f"{instructions}\n{procedure}",
    }


def provision_role(documents):
    directory = agent_directory(".config", "opencode")
    instructions = documents["instructions"].encode()
    write_private(directory / "AGENTS.md", instructions)
    role = documents["role"]
    if role not in {"rear-admiral", "captain", "crew"}:
        raise ValueError("Unknown fleet role.")
    skill = agent_directory(".config", "opencode", "skills", f"fleet-{role}")
    write_private(skill / "SKILL.md", documents["skill"].encode())
