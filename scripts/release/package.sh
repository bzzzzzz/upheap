#!/usr/bin/env bash
# Packs a built `upheap` binary into the release archive: a gzip'd tar with upheap, LICENSE-MIT, LICENSE-APACHE and
# README.md at the top level and no enclosing directory. Prints the archive's path.
#
# Usage: package.sh <target> <binary> <out-dir>
# Called by .github/workflows/release.yml (build job); also runnable on a Mac for a local dry run.
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <target> <binary> <out-dir>" >&2
  exit 2
fi
target=$1
binary=$2
out_dir=$3

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

if [ ! -f "$binary" ]; then
  echo "package.sh: binary not found: $binary" >&2
  exit 2
fi

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

install -m 755 "$binary" "$stage/upheap"
for f in LICENSE-MIT LICENSE-APACHE README.md; do
  install -m 644 "$root/$f" "$stage/$f"
done

mkdir -p "$out_dir"
out_dir=$(cd "$out_dir" && pwd)
archive="$out_dir/upheap-$target.tar.gz"
tar -czf "$archive" -C "$stage" upheap LICENSE-MIT LICENSE-APACHE README.md
echo "$archive"
