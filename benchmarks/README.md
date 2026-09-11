# Benchmarks

Speed means nothing if the output is wrong, so every timing is paired with
a correctness gate: a file counts only when pptxboss's report is empty and
its per-slide paragraphs agree with python-pptx after whitespace
normalization. Files any engine fails on are excluded from every row, and
the excluded files are listed with the reason.

## Text extraction (`bench.py`)

One adapter per engine, all called from Python. One warm-up pass, then
best-of-N per file, aggregated over the files every engine handled.
Metrics are slides per second and files per second; the JSON result also
records every engine's version and the machine.

```sh
pip install python-pptx office-oxide undoc kreuzberg markitdown
maturin develop --release
python benchmarks/bench.py /path/to/corpus --sample 200 --repeat 3
```

Engines: pptxboss (all cores), pptxboss-1t (`Document(path, threads=1)`,
one thread), office-oxide, undoc, kreuzberg, python-pptx, markitdown. The
other engines run one thread per file; office-oxide's wheel was measured
at 0.9 to 1.1 CPU seconds per wall second, so the pptxboss-1t row is the
like-for-like comparison. Both pptxboss rows include chart and SmartArt
text, which the other engines do not produce.

Last run (2026-09-11, Apple M3 Pro, `--repeat 3`, 737 files, 636 gated,
631 common, 1,370 slides): pptxboss 9,867.7 files/s; pptxboss-1t 7,941.7;
office-oxide 3,162.9; kreuzberg 1,865.8; undoc 1,831.0; python-pptx 488.2;
markitdown 71.1. No file was excluded for a paragraph disagreement;
`results.json` lists every exclusion with its reason. Absolute rates
depend on the cores macOS schedules the process on: an earlier session the
same day gave every engine 7x to 9x lower numbers, two runs in one session
differ by a few percent, and the ratios between engines hold within about
20%.

## Corpus

`corpora/fetch_public.sh DEST` fetches the `.pptx` test files of the
LibreOffice, Apache POI, python-pptx, pandoc and Open XML SDK repositories
(about 790 files) into a directory outside the repository. Nothing is
checked in; results record file counts and rates, never file names.

## Verifier calibration

`pptxboss check` was run over the same corpus plus two decks authored by
PowerPoint. The PowerPoint decks verify clean. Rules that fired on the
corpus fell into fuzzer-minimized garbage, hand-made test files missing
required parts, and files written by other libraries; those results set
the severities.
