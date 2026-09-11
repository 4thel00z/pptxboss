"""Text-extraction benchmark: pptxboss against the other Python-callable engines.

Every engine extracts the text of every file; a file counts only when every
engine handled it and pptxboss's own report is empty. Timing is best-of-N
per file after one warm-up pass, aggregated over the common files.
Correctness is gated separately: per-slide paragraph sets are compared
after normalization, and files where pptxboss disagrees with the majority
of the other engines are excluded and listed.

    python benchmarks/bench.py /path/to/corpus --sample 200 --repeat 3
    python benchmarks/bench.py /path/to/corpus --threads 1   # single-thread pptxboss
"""

from __future__ import annotations

import argparse
import importlib.metadata
import json
import os
import platform
import re
import sys
import time
from collections.abc import Callable
from pathlib import Path

Engine = Callable[[str], list[list[str]]]


def normalize(paragraph: str) -> str:
    return re.sub(r"\s+", " ", paragraph).strip()


def pptxboss_slides(path: str) -> list[list[str]]:
    import pptxboss

    doc = pptxboss.Document(path)
    return [[normalize(line) for p in slide.paragraphs() for line in p.split("\n") if normalize(line)] for slide in doc.slides()]


def pptxboss_text_one_thread(path: str) -> str:
    import pptxboss

    return pptxboss.Document(path, threads=1).text()


def pptxboss_text(path: str) -> str:
    import pptxboss

    return pptxboss.Document(path).text()


def python_pptx_slides(path: str) -> list[list[str]]:
    from pptx import Presentation

    out: list[list[str]] = []
    for slide in Presentation(path).slides:
        paragraphs: list[str] = []

        def walk(shapes) -> None:  # type: ignore[no-untyped-def]
            for shape in shapes:
                if shape.shape_type == 6:
                    walk(shape.shapes)
                    continue
                if shape.has_text_frame:
                    for p in shape.text_frame.paragraphs:
                        for line in p.text.replace("\x0b", "\n").split("\n"):
                            if normalize(line):
                                paragraphs.append(normalize(line))
                if getattr(shape, "has_table", False) and shape.has_table:
                    for row in shape.table.rows:
                        for cell in row.cells:
                            if cell.is_spanned:
                                continue
                            for p in cell.text_frame.paragraphs:
                                for line in p.text.replace("\x0b", "\n").split("\n"):
                                    if normalize(line):
                                        paragraphs.append(normalize(line))

        walk(slide.shapes)
        out.append(paragraphs)
    return out


def python_pptx_text(path: str) -> str:
    return "\n".join("\n".join(slide) for slide in python_pptx_slides(path))


def office_oxide_text(path: str) -> str:
    import office_oxide

    return office_oxide.extract_text(path)


def kreuzberg_text(path: str) -> str:
    import kreuzberg

    return kreuzberg.extract_file_sync(path).content


def undoc_text(path: str) -> str:
    import undoc

    return undoc.parse_file(path).to_text()


def markitdown_text(path: str) -> str:
    from markitdown import MarkItDown

    return MarkItDown().convert(path).text_content


TEXT_ENGINES: dict[str, tuple[str, Callable[[str], str]]] = {
    "pptxboss": ("pptxboss", pptxboss_text),
    "pptxboss-1t": ("pptxboss", pptxboss_text_one_thread),
    "office-oxide": ("office-oxide", office_oxide_text),
    "kreuzberg": ("kreuzberg", kreuzberg_text),
    "undoc": ("undoc", undoc_text),
    "python-pptx": ("python-pptx", python_pptx_text),
    "markitdown": ("markitdown", markitdown_text),
}


def version_of(dist: str) -> str | None:
    try:
        return importlib.metadata.version(dist)
    except importlib.metadata.PackageNotFoundError:
        return None


def sample(files: list[Path], count: int) -> list[Path]:
    if count <= 0 or count >= len(files):
        return files
    step = len(files) / count
    return [files[int(i * step)] for i in range(count)]


def time_one(fn: Callable[[str], object], path: str, repeat: int) -> float | None:
    best: float | None = None
    for _ in range(repeat):
        start = time.perf_counter()
        try:
            fn(path)
        except Exception:
            return None
        elapsed = time.perf_counter() - start
        best = elapsed if best is None else min(best, elapsed)
    return best


def correctness_gate(files: list[Path]) -> tuple[list[Path], list[tuple[Path, str]]]:
    """Keeps files where pptxboss reports nothing skipped and agrees with python-pptx paragraph for paragraph."""
    import pptxboss

    kept: list[Path] = []
    excluded: list[tuple[Path, str]] = []
    for path in files:
        try:
            doc = pptxboss.Document(str(path))
            _, warnings = doc.text_reporting()
            if warnings:
                excluded.append((path, "report: " + "; ".join(warnings[:2])))
                continue
            ours = pptxboss_slides(str(path))
        except Exception as err:
            excluded.append((path, f"pptxboss failed: {type(err).__name__}: {err}"))
            continue
        try:
            theirs = python_pptx_slides(str(path))
        except Exception as err:
            excluded.append((path, f"python-pptx failed: {type(err).__name__}"))
            continue
        if len(ours) != len(theirs):
            excluded.append((path, f"slide count {len(ours)} vs {len(theirs)}"))
            continue
        mismatch = next(((i, a, b) for i, (a, b) in enumerate(zip(ours, theirs)) if sorted(a) != sorted(b)), None)
        if mismatch is not None:
            i, a, b = mismatch
            only_ours = [x for x in a if x not in b][:2]
            only_theirs = [x for x in b if x not in a][:2]
            excluded.append((path, f"slide {i + 1}: only pptxboss={only_ours} only python-pptx={only_theirs}"))
            continue
        kept.append(path)
    return kept, excluded


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("corpus", type=Path)
    parser.add_argument("--sample", type=int, default=0, help="evenly spaced subset of files (default: all)")
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--engines", default=",".join(TEXT_ENGINES), help="comma-separated subset")
    parser.add_argument("--out", type=Path, default=Path(__file__).with_name("results.json"))
    parser.add_argument("--no-gate", action="store_true", help="skip the correctness gate")
    args = parser.parse_args()

    files = sorted(p for p in args.corpus.rglob("*.pptx") if p.is_file())
    files = sample(files, args.sample)
    if not files:
        sys.exit(f"no .pptx files under {args.corpus}")
    print(f"{len(files)} files from {args.corpus}")

    if args.no_gate:
        gated, excluded = files, []
    else:
        gated, excluded = correctness_gate(files)
        for path, reason in excluded:
            print(f"  excluded {path.name}: {reason}")
        print(f"{len(gated)} files pass the correctness gate ({len(excluded)} excluded)")

    engines = {name: TEXT_ENGINES[name] for name in args.engines.split(",") if name in TEXT_ENGINES}
    timings: dict[str, dict[str, float]] = {name: {} for name in engines}
    for name, (_, fn) in engines.items():
        for path in gated:
            try:
                fn(str(path))
            except Exception:
                pass
    for name, (_, fn) in engines.items():
        for path in gated:
            best = time_one(fn, str(path), args.repeat)
            if best is not None:
                timings[name][str(path)] = best
    common = set(str(p) for p in gated)
    for name in engines:
        common &= set(timings[name])
    slides = {}
    import pptxboss

    for path in common:
        slides[path] = pptxboss.Document(path).slide_count
    total_slides = sum(slides.values())

    rows = []
    for name in engines:
        total = sum(timings[name][path] for path in common)
        rows.append((name, total, len(common), total_slides / total if total else 0.0, len(common) / total if total else 0.0))
    rows.sort(key=lambda row: row[1])
    print()
    print(f"{'engine':14s} {'version':10s} {'files':>6s} {'slides/s':>10s} {'files/s':>9s} {'total s':>8s}")
    versions = {name: version_of(dist) for name, (dist, _) in engines.items()}
    for name, total, count, slides_per_s, files_per_s in rows:
        print(f"{name:14s} {str(versions[name] or '?'):10s} {count:6d} {slides_per_s:10.1f} {files_per_s:9.1f} {total:8.3f}")

    result = {
        "corpus": args.corpus.name,
        "files": len(files),
        "gated": len(gated),
        "common": len(common),
        "slides": total_slides,
        "repeat": args.repeat,
        "machine": {"platform": platform.platform(), "machine": platform.machine(), "python": platform.python_version(), "cpus": os.cpu_count()},
        "versions": versions,
        "excluded": [{"file": p.name, "reason": reason} for p, reason in excluded],
        "engines": {name: {"total_seconds": total, "files": count, "slides_per_second": sps, "files_per_second": fps} for name, total, count, sps, fps in rows},
    }
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(f"\nwrote {args.out}")


if __name__ == "__main__":
    main()
