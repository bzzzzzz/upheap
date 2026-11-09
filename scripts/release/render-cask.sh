#!/usr/bin/env bash
# Prints the Homebrew cask for a release: packaging/homebrew/upheap.rb with its version and the macOS archive's
# SHA-256 filled in. Exits 2 on a malformed version or checksum.
#
# Usage: render-cask.sh <version> <sha256>
# Called by .github/workflows/release.yml (cask job); also runnable locally.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <version> <sha256>" >&2
  exit 2
fi
version=$1
sha256=$2

if ! [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "render-cask.sh: version must look like 1.2.3, got '$version'" >&2
  exit 2
fi
if ! [[ $sha256 =~ ^[0-9a-f]{64}$ ]]; then
  echo "render-cask.sh: sha256 must be 64 lowercase hex characters, got '$sha256'" >&2
  exit 2
fi

template=$(dirname "${BASH_SOURCE[0]}")/../../packaging/homebrew/upheap.rb
sed -e "/^#/d" -e "s/@VERSION@/$version/" -e "s/@SHA256@/$sha256/" "$template"
