"""Every name the extension exports has a stub entry, and the package re-exports it."""

import ast
import re
from pathlib import Path

import pptxboss
from pptxboss import _pptxboss

STUB = Path(__file__).parents[1] / "python" / "pptxboss" / "_pptxboss.pyi"


def stub_names() -> set[str]:
    tree = ast.parse(STUB.read_text())
    names: set[str] = set()
    for node in tree.body:
        if isinstance(node, (ast.ClassDef, ast.FunctionDef)):
            names.add(node.name)
        if isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            names.add(node.target.id)
    return names


def test_every_exported_class_has_a_stub() -> None:
    exported = {name for name in dir(_pptxboss) if not name.startswith("_") or name == "__version__"}
    exported.discard("_pptxboss")
    missing = exported - stub_names()
    assert not missing, f"missing from _pptxboss.pyi: {sorted(missing)}"


def test_package_reexports_match_all() -> None:
    for name in pptxboss.__all__:
        assert hasattr(pptxboss, name)
    assert set(pptxboss.__all__) >= {"Document", "Slide", "PptxError"}


def test_every_public_method_has_a_stub_entry() -> None:
    text = STUB.read_text()
    for cls in (pptxboss.Document, pptxboss.Slide, pptxboss.Shape, pptxboss.Image):
        for name in dir(cls):
            if name.startswith("_"):
                continue
            assert re.search(rf"\b{name}\b", text), f"{cls.__name__}.{name} missing from stub"
