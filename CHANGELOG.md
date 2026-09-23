# Changelog

## [2.2.0](https://github.com/4thel00z/pptxboss/compare/v2.1.0...v2.2.0) (2026-09-23)


### Features

* **write:** compress embedded fonts as MicroType Express ([#16](https://github.com/4thel00z/pptxboss/issues/16)) ([550cd16](https://github.com/4thel00z/pptxboss/commit/550cd1686606cb40151ff1ffb4c8b4f6b9c15d39))

## [2.1.0](https://github.com/4thel00z/pptxboss/compare/v2.0.0...v2.1.0) (2026-09-23)


### Features

* **write:** embed font files in the deck ([#14](https://github.com/4thel00z/pptxboss/issues/14)) ([e186911](https://github.com/4thel00z/pptxboss/commit/e1869115be8e6e6d3f0f35d8abdde5a3ab0d7ffe))

## [2.0.0](https://github.com/4thel00z/pptxboss/compare/v1.1.0...v2.0.0) (2026-09-23)


### ⚠ BREAKING CHANGES

* **write:** layout engine: blocks, columns, fitting and continuation slides ([#12](https://github.com/4thel00z/pptxboss/issues/12))

### Features

* **write:** layout engine: blocks, columns, fitting and continuation slides ([#12](https://github.com/4thel00z/pptxboss/issues/12)) ([a8cd3f6](https://github.com/4thel00z/pptxboss/commit/a8cd3f6201d5d113242ccc439cb6b9d963cf99c3))

## [1.1.0](https://github.com/4thel00z/pptxboss/compare/v1.0.0...v1.1.0) (2026-09-21)


### Features

* **write:** accent title slides, theme-safe tables, Title layout from a subtitle, styled example ([#11](https://github.com/4thel00z/pptxboss/issues/11)) ([197b227](https://github.com/4thel00z/pptxboss/commit/197b227130716d532709e22dfa98ae6e3f210a84))
* **write:** seven more theme presets: midnight, mocha, dracula, nord, tokyo, clay, mono ([#9](https://github.com/4thel00z/pptxboss/issues/9)) ([2c3c492](https://github.com/4thel00z/pptxboss/commit/2c3c492dee0e08b360a06cfa7bcc1c241377e30a))

## [1.0.0](https://github.com/4thel00z/pptxboss/compare/v0.3.0...v1.0.0) (2026-09-21)


### ⚠ BREAKING CHANGES

* **write:** runs, themes and backgrounds ([#8](https://github.com/4thel00z/pptxboss/issues/8))

### Features

* **cli:** skill install parity with pdfboss, skills.sh route ([#6](https://github.com/4thel00z/pptxboss/issues/6)) ([b5fa0f3](https://github.com/4thel00z/pptxboss/commit/b5fa0f3aa5c49aec0636193a9de8ab632c60ecb9))
* **write:** runs, themes and backgrounds ([#8](https://github.com/4thel00z/pptxboss/issues/8)) ([4495915](https://github.com/4thel00z/pptxboss/commit/44959153aa9141ac33629f576a070881a32d7881))


### Documentation

* **book:** themed mdBook, canonical links, sitemap and README docs links ([40827b1](https://github.com/4thel00z/pptxboss/commit/40827b172adfb1803177f5479adcb4edad15945b))

## [0.3.0](https://github.com/4thel00z/pptxboss/compare/v0.2.1...v0.3.0) (2026-09-11)


### Features

* **py:** expose the rest of the Rust API to Python ([#4](https://github.com/4thel00z/pptxboss/issues/4)) ([1b6e440](https://github.com/4thel00z/pptxboss/commit/1b6e440bc0229d6faecd5e83034d860cf1c7afc1))

## [0.2.1](https://github.com/4thel00z/pptxboss/compare/v0.2.0...v0.2.1) (2026-09-11)


### Documentation

* plain wording in the README and the book, terminal screenshots ([#2](https://github.com/4thel00z/pptxboss/issues/2)) ([f82b048](https://github.com/4thel00z/pptxboss/commit/f82b04890b885c72f1d5bd5d62a3799ceb4795b5))

## [0.2.0](https://github.com/4thel00z/pptxboss/compare/v0.1.0...v0.2.0) (2026-09-11)


### Features

* chart and diagram text, Markdown output ([49d4fa0](https://github.com/4thel00z/pptxboss/commit/49d4fa053c16a14d63aa91cf11a235da2ef4d5b7))
* clean-room PresentationML reader with info and text commands ([7483688](https://github.com/4thel00z/pptxboss/commit/7483688debc53e27d1fe26361f3eb01f0dae8b6c))
* **cli:** --slides RANGE on info, text and markdown ([f965bfe](https://github.com/4thel00z/pptxboss/commit/f965bfe17a53e524065f549e3f2d008055c1db64))
* deck writer, Markdown to slides, create command, faster tokenizer, docs ([756f643](https://github.com/4thel00z/pptxboss/commit/756f64397848c08d48fec7d10d1e65800e8169c0))
* legacy .ppt reader through the same Document API ([a009549](https://github.com/4thel00z/pptxboss/commit/a0095498ddb801700c853b9cc27b6b7537b588cc))
* skill command, Python write bindings, corpus benchmark results ([f8b03c2](https://github.com/4thel00z/pptxboss/commit/f8b03c2f04e1665956d2aa6637e4c6af9c8c5df5))
* UTF-16 parts, interleaved pieces, comments, sections, properties, objects, alt text ([0040011](https://github.com/4thel00z/pptxboss/commit/0040011b2552c23631b0535451915508c6906d37))
* verifier, Python bindings, directory reconstruction and benchmarks ([c0bbc06](https://github.com/4thel00z/pptxboss/commit/c0bbc06ac58b6bdeedaca2eff46758ad444fcd74))


### Bug Fixes

* mixed-quote start tags, dead parameters, deflate error propagation ([87b96a0](https://github.com/4thel00z/pptxboss/commit/87b96a08cd57d694c81a5fbeb1de425532f4a89f))


### Performance Improvements

* one read per part, in-tree inflate, lazy content types, thread cap ([1da2b5c](https://github.com/4thel00z/pptxboss/commit/1da2b5c831f85c8b5bcad8bc75812fb23286f2a3))


### Documentation

* benchmark figures from a fast-core session, legacy decks on one thread ([8accef2](https://github.com/4thel00z/pptxboss/commit/8accef201991cd7e5b13f162cd82e9f542619993))

## Changelog
