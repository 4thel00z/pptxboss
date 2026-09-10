# Benchmarks

Speed means nothing if the output is wrong, so every timing here is paired
with a correctness gate: a file only counts when pptxboss's report is empty
and its per-slide text agrees with the majority of the other engines after
normalization. Files any engine fails on are excluded from every row.

The scripts, the competitor list, the corpus fetchers and the results
follow once the Python extension exists. Until then, the CLI can be timed
directly:

```sh
cargo build --release -p pptxboss-cli
time target/release/pptxboss text deck.pptx > /dev/null
```
