#!/usr/bin/env python3
"""Validate the Copilot skills directory.

Checks, for everything under ``skills/``:

1. each ``skills/<name>/SKILL.md`` frontmatter has ``name`` == folder, contains
   no angle brackets, and a ``description`` of at most 1024 characters;
2. every ``references/*.md`` file is listed in its parent ``SKILL.md``;
3. every relative ``.md`` link (in backticks or markdown links) resolves;
4. every ``SKILL.md`` references the shared ``SKILL_ROUTER.md``.

Exits non-zero on any failure so CI fails loudly.
"""
from __future__ import annotations

import glob
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SKILLS = os.path.join(ROOT, "skills")

errors: list[str] = []


def rel(p: str) -> str:
    return os.path.relpath(p, ROOT)


def check_frontmatter() -> None:
    for f in sorted(glob.glob(os.path.join(SKILLS, "*", "SKILL.md"))):
        skill = os.path.basename(os.path.dirname(f))
        txt = open(f, encoding="utf-8").read()
        parts = txt.split("---")
        if len(parts) < 3:
            errors.append(f"{rel(f)}: missing YAML frontmatter")
            continue
        fm = parts[1]
        m_name = re.search(r"^name:\s*(.+)$", fm, re.M)
        m_desc = re.search(r"^description:\s*(.+)$", fm, re.M)
        if not m_name or not m_desc:
            errors.append(f"{rel(f)}: frontmatter needs both name and description")
            continue
        name = m_name.group(1).strip()
        desc = m_desc.group(1).strip()
        if name != skill:
            errors.append(f"{rel(f)}: name '{name}' != folder '{skill}'")
        if "<" in fm or ">" in fm:
            errors.append(f"{rel(f)}: frontmatter contains angle brackets")
        if len(desc) > 1024:
            errors.append(f"{rel(f)}: description is {len(desc)} chars (max 1024)")
        if "SKILL_ROUTER" not in txt:
            errors.append(f"{rel(f)}: does not reference SKILL_ROUTER.md")


def check_reference_listing() -> None:
    for skilldir in sorted(glob.glob(os.path.join(SKILLS, "*") + os.sep)):
        sk = os.path.join(skilldir, "SKILL.md")
        if not os.path.exists(sk):
            continue
        body = open(sk, encoding="utf-8").read()
        for r in sorted(glob.glob(os.path.join(skilldir, "references", "*.md"))):
            token = "references/" + os.path.basename(r)
            if token not in body:
                errors.append(f"{rel(sk)}: reference {token} is not listed")


def check_links() -> None:
    link_re = re.compile(r"`(\.\.?/[^`]+?\.md)`|\]\((\.\.?/[^)]+?\.md)\)")
    for f in glob.glob(os.path.join(SKILLS, "**", "*.md"), recursive=True):
        base = os.path.dirname(f)
        for m in link_re.findall(open(f, encoding="utf-8").read()):
            link = m[0] or m[1]
            target = os.path.normpath(os.path.join(base, link))
            if not os.path.exists(target):
                errors.append(f"{rel(f)}: broken link -> {link}")


def main() -> int:
    if not os.path.isdir(SKILLS):
        print("no skills/ directory found", file=sys.stderr)
        return 1
    check_frontmatter()
    check_reference_listing()
    check_links()
    if errors:
        print("Skill validation FAILED:\n")
        for e in errors:
            print(f"  - {e}")
        return 1
    n_skills = len(glob.glob(os.path.join(SKILLS, "*", "SKILL.md")))
    n_refs = len(glob.glob(os.path.join(SKILLS, "*", "references", "*.md")))
    print(f"Skill validation passed: {n_skills} skills, {n_refs} references, all links resolve.")
    return 0


if __name__ == "__main__":
    sys.exit(main())