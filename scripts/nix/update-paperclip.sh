#!/usr/bin/env bash

set -eu -o pipefail

SCRIPTDIR=$(dirname "$0")
ROOTDIR="$SCRIPTDIR"/../..

OWNER="paperclip-rs";
REPO="paperclip";
REV=${REV:-}
ARCH_OS=("x86_64-unknown-linux-musl" "aarch64-unknown-linux-musl" "x86_64-apple-darwin" "aarch64-apple-darwin")
ARCH_OS_SHA256s=""


github_rel_tag() {
  curl -sSf "https://api.github.com/repos/$OWNER/$REPO/releases/latest" | jq -r '.tag_name'
}

github_sha256() {
  tag="$1"
  arch_os="$2"
  nix-prefetch-url \
     --type sha256 \
     "https://github.com/$OWNER/$REPO/releases/download/$tag/paperclip-$arch_os.tar.gz" 2>&1 | \
     tail -1
}

echo "=== $OWNER/$REPO ==="

echo -n "Looking up latest release for $OWNER/$REPO... "
if [ -z "$REV" ]; then
  tag=$(github_rel_tag);
else
  tag="$REV"
fi
echo "$tag"

for arch_os in "${ARCH_OS[@]}"; do
  echo -n "Looking up sha25 for $arch_os... "
  sha256=$(github_sha256 "$tag" "$arch_os");
  echo "$sha256"
  if [ "$ARCH_OS_SHA256s" != "" ]; then
    ARCH_OS_SHA256s="$ARCH_OS_SHA256s,
    "
  fi
  ARCH_OS_SHA256s="$ARCH_OS_SHA256s\"$arch_os\": \"$sha256\""
done

source_file="$ROOTDIR/nix/pkgs/paperclip/source.json"

echo "Previous Content of source file (``$source_file``):"
cat "$source_file"
echo "New Content of source file (``$source_file``) written."
cat <<EOF >$source_file
{
  "owner": "$OWNER",
  "repo": "$REPO",
  "rev": "$tag",
  "hash": {
    $ARCH_OS_SHA256s
  }
}
EOF
echo
