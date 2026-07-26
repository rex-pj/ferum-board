#!/usr/bin/env bash
# Rebuilds every distributable archive under examples/ from its source directory.
#
#   Themes  -> examples/themes/<dir>/   ==> examples/themes/<dir>-<version>.zip
#   Plugins -> examples/plugins/<dir>/  ==> examples/plugins/<dir>-<version>.fpkg
#
# POSIX counterpart to package-examples.ps1, for Linux/macOS development and CI.
# The two must stay behaviourally identical: same archive layout (source
# directory CONTENTS at the archive root, so theme.json / plugin.toml sit at the
# top level), same validation, same --check semantics.
#
# Usage:
#   scripts/package-examples.sh            # rebuild
#   scripts/package-examples.sh --check    # verify only; non-zero exit if stale
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
check_only=0
[ "${1:-}" = "--check" ] && check_only=1

failures=()
built=0
discovered=0   # guards against a vacuous success — see the check at the end

# Extensions the theme upload handler refuses. Catching them here turns a
# confusing upload-time rejection into a build-time error naming the file.
forbidden_ext=("svelte" "ts" "tsx" "jsx" "vue")

have() { command -v "$1" >/dev/null 2>&1; }

if have zip && have unzip; then
  :
else
  echo "error: this script needs both 'zip' and 'unzip' on PATH." >&2
  echo "       Debian/Ubuntu: sudo apt-get install zip unzip" >&2
  echo "       Fedora:        sudo dnf install zip unzip" >&2
  echo "       macOS:         both ship with the system." >&2
  exit 2
fi

sha_of() {
  if have sha256sum; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1   # macOS
  fi
}

# Relative paths of every file in a directory, sorted — the archive's file list.
src_files() { (cd "$1" && find . -type f | sed 's|^\./||' | LC_ALL=C sort); }

# Entry names inside an archive, sorted, directories excluded.
arc_files() { unzip -Z1 "$1" 2>/dev/null | grep -v '/$' | LC_ALL=C sort; }

# "<relpath>  <sha256>" per file, for content comparison.
#
# Comparing names alone is what let stale archives survive review: two theme
# archives held exactly the files their sources held and differed only inside
# templates/base.html. A check that cannot see that is worse than no check,
# because it reports success.
src_hashes() {
  local d="$1"
  (cd "$d" && find . -type f | sed 's|^\./||' | LC_ALL=C sort) | while IFS= read -r f; do
    printf '%s  %s\n' "$f" "$(sha_of "$d/$f")"
  done
}
arc_hashes() {
  local a="$1" tmp
  tmp="$(mktemp -d)"
  unzip -qq -o "$a" -d "$tmp" 2>/dev/null
  src_hashes "$tmp"
  rm -rf "$tmp"
}

build_archive() {
  local src="$1" out="$2" label="$3"
  discovered=$((discovered + 1))

  local n; n="$(src_files "$src" | wc -l | tr -d ' ')"
  if [ "$n" -eq 0 ]; then failures+=("$label - source directory is empty"); return; fi

  if [ "$check_only" -eq 1 ]; then
    if [ ! -f "$out" ]; then
      failures+=("$label - archive missing: $(basename "$out")"); return
    fi
    if ! diff -q <(src_files "$src") <(arc_files "$out") >/dev/null; then
      local d; d="$(diff <(src_files "$src") <(arc_files "$out") | grep '^[<>]' | tr '\n' ' ')"
      failures+=("$label - file list differs: $d"); return
    fi
    if ! diff -q <(src_hashes "$src") <(arc_hashes "$out") >/dev/null; then
      local c; c="$(diff <(src_hashes "$src") <(arc_hashes "$out") | grep '^<' | awk '{print $2}' | tr '\n' ' ')"
      failures+=("$label - content differs: $c"); return
    fi
    echo "  ok    $label ($n files)"
    return
  fi

  # Rebuild from scratch: updating in place would leave files deleted from the
  # source still present in the archive, which is exactly how these drifted.
  rm -f "$out"
  (cd "$src" && zip -q -X -r "$out" . -x '.*') || { failures+=("$label - zip failed"); return; }

  if ! diff -q <(src_files "$src") <(arc_files "$out") >/dev/null; then
    failures+=("$label - archive does not match source after write"); return
  fi
  local kb; kb="$(du -k "$out" | cut -f1)"
  echo "  built $label -> $(basename "$out") ($n files, ${kb} KB)"
  built=$((built + 1))
}

json_value() { grep -oP "\"$2\"\s*:\s*\"\K[^\"]+" "$1" | head -1; }
toml_value() { grep -oP "^\s*$2\s*=\s*\"\K[^\"]+" "$1" | head -1; }

# ── Themes ────────────────────────────────────────────────────────────────────
echo
echo "Themes"
themes_root="$repo_root/examples/themes"
for dir in "$themes_root"/*/; do
  [ -d "$dir" ] || continue
  name="$(basename "$dir")"; label="theme/$name"
  manifest="$dir/theme.json"

  [ -f "$manifest" ] || { failures+=("$label - missing theme.json"); continue; }
  # The upload handler rejects an archive without it.
  [ -f "$dir/templates/base.html" ] || { failures+=("$label - missing templates/base.html"); continue; }

  bad=""
  for ext in "${forbidden_ext[@]}"; do
    found="$(find "$dir" -type f -name "*.$ext" -printf '%f ' 2>/dev/null)"
    [ -n "$found" ] && bad="$bad$found"
  done
  [ -n "$bad" ] && { failures+=("$label - contains file types the uploader rejects: $bad"); continue; }

  version="$(json_value "$manifest" version)"
  [ -n "$version" ] || { failures+=("$label - theme.json has no version"); continue; }

  build_archive "$dir" "$themes_root/$name-$version.zip" "$label"
done

# ── Plugins ───────────────────────────────────────────────────────────────────
echo
echo "Plugins"
plugins_root="$repo_root/examples/plugins"
for dir in "$plugins_root"/*/; do
  [ -d "$dir" ] || continue
  name="$(basename "$dir")"; label="plugin/$name"
  manifest="$dir/plugin.toml"

  [ -f "$manifest" ] || { failures+=("$label - missing plugin.toml"); continue; }

  version="$(toml_value "$manifest" version)"
  [ -n "$version" ] || { failures+=("$label - plugin.toml has no version"); continue; }

  # A Script-tier plugin names its bundle; a Manifest-tier one has no code at
  # all. Only verify the file exists when the manifest actually points at one.
  bundle="$(toml_value "$manifest" bundle_file)"
  if [ -n "$bundle" ] && [ ! -f "$dir/$bundle" ]; then
    failures+=("$label - plugin.toml references missing bundle '$bundle'"); continue
  fi

  build_archive "$dir" "$plugins_root/$name-$version.fpkg" "$label"
done

# ── Result ────────────────────────────────────────────────────────────────────
echo
# Finding nothing must never read as success. A wrong repo_root (running the
# script through a copy, a symlink, or a renamed checkout) makes both globs
# match zero directories, and without this the script would cheerfully report
# "All archives match their sources" having compared nothing at all.
if [ "$discovered" -eq 0 ]; then
  echo "FAILED:" >&2
  echo "  - no theme or plugin directories found under $repo_root/examples/" >&2
  echo "    (is repo_root wrong? expected examples/themes/ and examples/plugins/)" >&2
  exit 1
fi

if [ "${#failures[@]}" -gt 0 ]; then
  echo "FAILED:"
  for f in "${failures[@]}"; do echo "  - $f"; done
  exit 1
fi
if [ "$check_only" -eq 1 ]; then echo "All archives match their sources."
else echo "Rebuilt $built archive(s)."; fi
