#!/usr/bin/env bash
# Build a publishable copy of this repository's history in a NEW directory.
#
#   - clones ONE branch (default: main) with --no-local, so no other branch, worktree,
#     stash or reflog of the source repo comes along
#   - rewrites author/committer identities (mailmap) and redacts private strings in every
#     commit (git filter-repo --replace-text)
#   - verifies the result and prints a report; it NEVER pushes and never touches the source repo
#
# Required environment:
#   PUBLIC_EMAIL   address every old identity is mapped to, normally your GitHub noreply address
#                  (<id>+<login>@users.noreply.github.com)
#   OLD_EMAILS     comma-separated list of the old author/committer emails to replace
# Optional environment:
#   PUBLIC_NAME    public author name (default: keep each commit's existing name)
#   BRANCH         branch to publish (default: main)
#   REDACT_FILE    extra filter-repo --replace-text rules (default: scripts/release/redact.local.txt
#                  if it exists; that file is git-ignored because it holds the private strings)
#
# Usage:  PUBLIC_EMAIL=... OLD_EMAILS=a@x,b@y scripts/release/public_history.sh /path/to/new-dir
#
# Requires: git, git-filter-repo. Uses gitleaks when installed (strongly recommended).
set -euo pipefail

die() { echo "error: $*" >&2; exit 1; }

OUT="${1:-}"
[[ -n "$OUT" ]] || die "usage: $0 <new-empty-directory>"
: "${PUBLIC_EMAIL:?set PUBLIC_EMAIL (your GitHub noreply address)}"
: "${OLD_EMAILS:?set OLD_EMAILS (comma-separated old author emails)}"
BRANCH="${BRANCH:-main}"
PUBLIC_NAME="${PUBLIC_NAME:-}"

SRC="$(git rev-parse --show-toplevel)"
SRC_COMMON="$(git rev-parse --git-common-dir)"
SRC_COMMON="$(cd "$SRC_COMMON" && pwd)"
command -v git-filter-repo >/dev/null 2>&1 || die "git-filter-repo not found (brew install git-filter-repo)"
git rev-parse --verify --quiet "refs/heads/$BRANCH" >/dev/null || die "branch '$BRANCH' not found in $SRC"

[[ -d "$(dirname "$OUT")" ]] || die "parent directory of $OUT does not exist"
OUT="$(cd "$(dirname "$OUT")" && pwd)/$(basename "$OUT")"
case "$OUT" in "$SRC"|"$SRC"/*|"$SRC_COMMON"|"$SRC_COMMON"/*) die "output must be outside the source repository" ;; esac
mkdir -p "$OUT"
[[ -z "$(ls -A "$OUT")" ]] || die "$OUT is not empty"

REDACT_FILE="${REDACT_FILE:-$SRC/scripts/release/redact.local.txt}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/public-history.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

echo "[1/5] cloning $BRANCH from $SRC (no hardlinks, single branch)"
git clone --quiet --no-local --single-branch --branch "$BRANCH" "$SRC" "$OUT"
SRC_TIP="$(git -C "$SRC" rev-parse "refs/heads/$BRANCH")"
SRC_COUNT="$(git -C "$SRC" rev-list --count "refs/heads/$BRANCH")"

echo "[2/5] writing rewrite rules"
: > "$WORK/mailmap"
IFS=',' read -r -a OLD <<< "$OLD_EMAILS"
for e in "${OLD[@]}"; do
  e="${e// /}"; [[ -n "$e" ]] || continue
  if [[ -n "$PUBLIC_NAME" ]]; then
    echo "$PUBLIC_NAME <$PUBLIC_EMAIL> <$e>" >> "$WORK/mailmap"
  else
    # keep each commit's own name: map email only
    echo "<$PUBLIC_EMAIL> <$e>" >> "$WORK/mailmap"
  fi
done
cat > "$WORK/replace.txt" <<'EOF'
regex:192\.168\.\d{1,3}\.\d{1,3}==><lan-ip>
regex:/Users/[A-Za-z0-9._-]+/==>/Users/<user>/
EOF
if [[ -f "$REDACT_FILE" ]]; then
  cat "$REDACT_FILE" >> "$WORK/replace.txt"
  echo "      + extra rules from $REDACT_FILE"
else
  echo "      (no REDACT_FILE: only the generic LAN-IP and home-path rules apply)"
fi

echo "[3/5] rewriting history in $OUT"
git -C "$OUT" filter-repo --quiet --force \
  --mailmap "$WORK/mailmap" \
  --replace-text "$WORK/replace.txt"

echo "[4/5] verifying"
fail=0
NEW_COUNT="$(git -C "$OUT" rev-list --count HEAD)"
[[ "$NEW_COUNT" == "$SRC_COUNT" ]] || { echo "  FAIL commit count $SRC_COUNT -> $NEW_COUNT"; fail=1; }
echo "  commits: $NEW_COUNT (source $SRC_COUNT)"

# the file set must be identical; only contents may differ
if diff <(git -C "$SRC" ls-tree -r --name-only "$SRC_TIP") <(git -C "$OUT" ls-tree -r --name-only HEAD) >/dev/null; then
  echo "  file list: identical to source tip"
else
  echo "  FAIL file list differs from source tip"; fail=1
fi
echo "  files whose content changed at the tip:"
git -C "$OUT" fetch --quiet "$SRC" "refs/heads/$BRANCH" 2>/dev/null || true
git -C "$OUT" diff --stat FETCH_HEAD HEAD 2>/dev/null | tail -n 1 | sed 's/^/    /' || true

echo "  authors/committers after rewrite:"
git -C "$OUT" log --format='%an <%ae>%n%cn <%ce>' | sort | uniq -c | sort -rn | sed 's/^/    /'
# NOTE: never use `grep -q` after a pipe here: with pipefail it closes the pipe early and the
# pipeline reports failure, which silently turned "found" into "not found".
for e in "${OLD[@]}"; do
  e="${e// /}"; [[ -n "$e" ]] || continue
  n_meta="$(git -C "$OUT" log --format='%ae %ce %b' | grep -ciF -- "$e" || true)"
  if [[ "$n_meta" != 0 ]]; then echo "  FAIL old email still in $n_meta commits' metadata"; fail=1; fi
  if [[ -n "$(git -C "$OUT" log --all -p --format= -S"$e" | head -n 1)" ]]; then
    echo "  FAIL old email still in file contents of some commit"; fail=1
  fi
done

leak_patterns=('192\.168\.[0-9]+\.[0-9]+' '/Users/[A-Za-z0-9._-]+/[^<]')
for p in "${leak_patterns[@]}"; do
  n="$(git -C "$OUT" log --all -p --format= -G"$p" | grep -cE "^\+.*$p" || true)"
  if [[ "$n" == 0 ]]; then echo "  clean: $p"; else echo "  FAIL $n added lines still match $p"; fail=1; fi
done
if [[ -f "$REDACT_FILE" ]]; then
  while IFS= read -r line; do
    lit="${line%%==>*}"
    [[ -z "$lit" || "$lit" == regex:* || "$lit" == \#* ]] && continue
    if [[ -n "$(git -C "$OUT" log --all -p --format= -S"$lit" | head -n 1)" ]]; then
      echo "  FAIL a literal from REDACT_FILE is still present"; fail=1
    fi
  done < "$REDACT_FILE"
fi
echo "  remote configured on the new repo: $(git -C "$OUT" remote | tr '\n' ' ')(none expected)"

echo "[5/5] secret scan"
if command -v gitleaks >/dev/null 2>&1; then
  if gitleaks detect --source "$OUT" --redact --no-banner --exit-code 1; then echo "  gitleaks: no findings"; else echo "  FAIL gitleaks reported findings (see above)"; fail=1; fi
else
  echo "  WARNING: gitleaks not installed — NO secret scan was done. Install it (brew install gitleaks) and run:"
  echo "           gitleaks detect --source '$OUT' --redact"
fi

echo
if (( fail )); then
  echo "RESULT: FAILED — do not push. Fix the rules/inputs, delete '$OUT' and run again."
  exit 1
fi
cat <<EOF
RESULT: OK. Nothing was pushed. To publish to the PRIVATE repo for review:

  cd '$OUT'
  git remote add origin git@github.com:<owner>/rabbiz-os-project.git
  git push -u origin $BRANCH

Review the repo in the GitHub UI before making it public. Keep the original repository as a private backup.
EOF
