#!/usr/bin/env sh

set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: sh scripts/bump-version.sh <major|feature|fix>" >&2
  exit 2
fi

kind="$1"
repo_root="$(git rev-parse --show-toplevel)"
version_file="$repo_root/VERSION"
current="$(sh "$repo_root/scripts/validate-version.sh" "$version_file")"

major="${current%%.*}"
remainder="${current#*.}"
minor="${remainder%%.*}"
patch="${remainder#*.}"

case "$kind" in
  major)
    major=$((major + 1))
    minor=0
    patch=0
    ;;
  feature | feat | minor)
    minor=$((minor + 1))
    patch=0
    ;;
  fix | bug | patch)
    patch=$((patch + 1))
    ;;
  *)
    echo "bump-version: unknown bump kind '$kind' (expected major|feature|fix)" >&2
    exit 2
    ;;
esac

next="$major.$minor.$patch"
printf '%s\n' "$next" > "$version_file"
echo "bump-version: $current -> $next ($kind)"