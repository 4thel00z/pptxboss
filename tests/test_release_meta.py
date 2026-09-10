"""Structural guards: version markers agree, the release workflow syncs the lockfile, skill copies match."""

import re
from pathlib import Path

import yaml

ROOT = Path(__file__).parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text()


def workspace_version() -> str:
    match = re.search(r'^version = "([^"]+)" # x-release-please-version$', read("Cargo.toml"), re.M)
    assert match, "workspace version marker missing"
    return match.group(1)


def test_versions_agree_everywhere() -> None:
    version = workspace_version()
    assert f'version = "{version}"' in read("pyproject.toml")
    manifest = read(".release-please-manifest.json")
    assert f'"{version}"' in manifest
    for line in read("Cargo.toml").splitlines():
        if "x-release-please-version" in line and "path =" in line:
            assert f'version = "{version}"' in line, line


def test_lockfile_pins_workspace_crates_at_the_workspace_version() -> None:
    version = workspace_version()
    lock = read("Cargo.lock")
    for crate in ("pptxboss-core", "pptxboss-check", "pptxboss-write", "pptxboss-cli", "pptxboss-py", "pptxboss-testkit"):
        block = re.search(rf'\[\[package\]\]\nname = "{crate}"\nversion = "([^"]+)"', lock)
        assert block, f"{crate} missing from Cargo.lock"
        assert block.group(1) == version, f"{crate} is {block.group(1)} in Cargo.lock, workspace is {version}"


def test_release_workflow_syncs_the_lockfile_and_publishes() -> None:
    workflow = yaml.safe_load(read(".github/workflows/release-please.yaml"))
    jobs = workflow["jobs"]
    assert "sync-lockfile" in jobs
    steps = " ".join(str(step.get("run", "")) for step in jobs["sync-lockfile"]["steps"])
    assert "cargo update --workspace" in steps
    assert "publish-pypi" in jobs and "publish-crates" in jobs
    assert any("pypa/gh-action-pypi-publish" in str(step.get("uses", "")) for step in jobs["publish-pypi"]["steps"])


def test_python_ci_installs_the_test_dependencies() -> None:
    workflow = yaml.safe_load(read(".github/workflows/python-ci.yml"))
    steps = " ".join(str(step.get("run", "")) for step in workflow["jobs"]["pytest"]["steps"])
    assert "pip install . pytest pyyaml" in steps


def test_skill_copies_are_identical_and_well_formed() -> None:
    embedded = read("crates/pptxboss-cli/skill/SKILL.md")
    mirror = read("skills/pptxboss/SKILL.md")
    assert embedded == mirror, "skills/pptxboss/SKILL.md must be byte-identical to crates/pptxboss-cli/skill/SKILL.md"
    assert embedded.startswith("---\nname: pptxboss\n")
    assert "description:" in embedded
    assert "—" not in embedded, "no em dashes in the skill"
