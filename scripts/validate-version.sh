#!/usr/bin/env sh

set -eu

if [ "$#" -gt 1 ]; then
  echo "usage: sh scripts/validate-version.sh [version-file]" >&2
  exit 2
fi

repo_root="$(git rev-parse --show-toplevel)"
version_file="${1:-$repo_root/VERSION}"

if [ ! -f "$version_file" ]; then
  echo "validate-version: VERSION file not found at $version_file" >&2
  exit 1
fi

version="$(cat "$version_file")"
major="${version%%.*}"
remainder="${version#*.}"
minor="${remainder%%.*}"
patch="${remainder#*.}"

is_version_component() {
  case "$1" in
    0 | [1-9]*[0-9] | [1-9])
      case "$1" in
        *[!0-9]*) return 1 ;;
        *) return 0 ;;
      esac
      ;;
    *) return 1 ;;
  esac
}

if [ "$remainder" = "$version" ] \
  || [ "$patch" = "$remainder" ] \
  || [ "${patch#*.}" != "$patch" ] \
  || ! is_version_component "$major" \
  || ! is_version_component "$minor" \
  || ! is_version_component "$patch"; then
  echo "validate-version: VERSION '$version' is not a valid X.Y.Z version" >&2
  exit 1
fi

printf '%s\n' "$version"