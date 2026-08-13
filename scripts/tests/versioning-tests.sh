#!/usr/bin/env sh

set -eu

source_root="$(git rev-parse --show-toplevel)"
test_root="$(mktemp -d)"
trap 'rm -rf "$test_root"' EXIT HUP INT TERM

mkdir -p "$test_root/.githooks" "$test_root/scripts"
cp "$source_root/.githooks/pre-commit" "$test_root/.githooks/pre-commit"
cp "$source_root/.githooks/pre-push" "$test_root/.githooks/pre-push"
cp "$source_root/scripts/bump-version.sh" "$test_root/scripts/bump-version.sh"
cp "$source_root/scripts/validate-version.sh" "$test_root/scripts/validate-version.sh"
git -C "$test_root" init --quiet
git -C "$test_root" config user.email "versioning-tests@example.invalid"
git -C "$test_root" config user.name "Versioning Tests"

assert_bump() {
  initial="$1"
  kind="$2"
  expected="$3"

  printf '%s\n' "$initial" > "$test_root/VERSION"
  (cd "$test_root" && sh scripts/bump-version.sh "$kind" >/dev/null)
  actual="$(tr -d '\r\n' < "$test_root/VERSION")"
  if [ "$actual" != "$expected" ]; then
    echo "versioning-tests: $kind changed $initial to $actual; expected $expected" >&2
    exit 1
  fi
}

assert_invalid() {
  invalid="$1"

  printf '%s\n' "$invalid" > "$test_root/VERSION"
  if (cd "$test_root" && sh scripts/validate-version.sh >/dev/null 2>&1); then
    echo "versioning-tests: invalid version '$invalid' was accepted" >&2
    exit 1
  fi
}

assert_bump "1.2.3" fix "1.2.4"
assert_bump "1.2.3" patch "1.2.4"
assert_bump "1.2.3" feature "1.3.0"
assert_bump "1.2.3" minor "1.3.0"
assert_bump "1.2.3" major "2.0.0"

assert_invalid ""
assert_invalid "1.2"
assert_invalid "1.2.3.4"
assert_invalid "01.2.3"
assert_invalid "1.02.3"
assert_invalid "1.2.03"
assert_invalid "1.2.x"
assert_invalid "1.2
.3"

printf '%s\n' "3.4.5" > "$test_root/VERSION"
git -C "$test_root" add VERSION
git -C "$test_root" commit --quiet --no-verify -m "test fixture"

(cd "$test_root" && TCO_BUMP=minor sh .githooks/pre-commit >/dev/null)
staged_version="$(git -C "$test_root" show :VERSION | tr -d '\r\n')"
if [ "$staged_version" != "3.5.0" ]; then
  echo "versioning-tests: minor hook staged $staged_version; expected 3.5.0" >&2
  exit 1
fi

git -C "$test_root" reset --quiet --hard HEAD
(cd "$test_root" && sh .githooks/pre-commit >/dev/null)
staged_version="$(git -C "$test_root" show :VERSION | tr -d '\r\n')"
if [ "$staged_version" != "3.4.6" ]; then
  echo "versioning-tests: default hook staged $staged_version; expected 3.4.6" >&2
  exit 1
fi

git -C "$test_root" reset --quiet --hard HEAD
if (cd "$test_root" && TCO_BUMP=typo sh .githooks/pre-commit >/dev/null 2>&1); then
  echo "versioning-tests: unknown TCO_BUMP was accepted" >&2
  exit 1
fi
if [ "$(tr -d '\r\n' < "$test_root/VERSION")" != "3.4.5" ]; then
  echo "versioning-tests: rejected TCO_BUMP changed VERSION" >&2
  exit 1
fi

(cd "$test_root" && sh .githooks/pre-push >/dev/null)
printf '%s\n' "3.4" > "$test_root/VERSION"
if (cd "$test_root" && sh .githooks/pre-push >/dev/null 2>&1); then
  echo "versioning-tests: pre-push accepted an invalid version" >&2
  exit 1
fi

printf '%s\n' "versioning-tests: all tests passed"