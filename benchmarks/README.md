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

Engines: pptxboss, office-oxide, undoc, kreuzberg, python-pptx,
markitdown. pptxboss spreads slides across cores; the others run one
thread per file.

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
