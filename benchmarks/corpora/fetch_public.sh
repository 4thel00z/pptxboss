#!/usr/bin/env bash
# Fetches the public .pptx test corpora used by several open-source projects
# into a directory outside the repository. Nothing is checked in.
#
#   benchmarks/corpora/fetch_public.sh ~/corpora/pptx
#
# Sources (each cloned shallowly with a sparse checkout of one directory):
#   LibreOffice core     sd/qa/unit/data/pptx
#   Apache POI           test-data/slideshow
#   python-pptx          tests/test_files, features/steps/test_files
#   pandoc               test/pptx
#   Open XML SDK samples samples
set -euo pipefail

dest="${1:?destination directory required}"
mkdir -p "$dest"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

fetch() {
    local name="$1" repo="$2"; shift 2
    echo "== $name"
    git clone --quiet --depth 1 --filter=blob:none --sparse "$repo" "$work/$name"
    (cd "$work/$name" && git sparse-checkout set --no-cone "$@" >/dev/null)
    local count=0
    while IFS= read -r -d '' file; do
        local base
        base="$(basename "$file")"
        cp "$file" "$dest/${name}__${base}"
        count=$((count + 1))
    done < <(find "$work/$name" -type f \( -iname '*.pptx' -o -iname '*.pptm' -o -iname '*.potx' -o -iname '*.ppsx' \) -print0)
    echo "   $count files"
    rm -rf "$work/$name"
}

fetch libreoffice https://github.com/LibreOffice/core.git 'sd/qa/unit/data/pptx/*'
fetch poi https://github.com/apache/poi.git 'test-data/slideshow/*'
fetch python-pptx https://github.com/scanny/python-pptx.git 'tests/test_files/*' 'features/steps/test_files/*'
fetch pandoc https://github.com/jgm/pandoc.git 'test/pptx/*'
fetch openxml-sdk https://github.com/dotnet/Open-XML-SDK.git 'samples/*' 'test/*'

echo "total: $(find "$dest" -type f | wc -l | tr -d ' ') files in $dest"
