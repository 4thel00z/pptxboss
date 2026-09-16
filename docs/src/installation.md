# Installation

## Python

```sh
pip install pptxboss
```

Wheels are built for Linux x86_64 and macOS arm64 against the stable ABI
for Python 3.12 and later. Other platforms build from the sdist with a Rust
toolchain present.

## Command line

```sh
cargo install pptxboss-cli
```

Coding agents get a bundled [skill](https://github.com/4thel00z/pptxboss/blob/main/skills/pptxboss/SKILL.md): `pptxboss skill install` writes it into `./.claude/skills/pptxboss/`, and `pptxboss skill install -g` into the home directory.

## Rust crates

```toml
[dependencies]
pptxboss-core = "0.2"    # reading
pptxboss-check = "0.2"   # verifying
pptxboss-write = "0.2"   # creating
```

## From source

```sh
git clone https://github.com/4thel00z/pptxboss
cd pptxboss
cargo test --workspace
python -m venv .venv && . .venv/bin/activate
pip install maturin pytest pyyaml
maturin develop && pytest
```
