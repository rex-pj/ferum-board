#!/bin/sh
# Re-sync the built-in themes into THEMES_DIR before starting the server.
#
# ─── Why this exists ────────────────────────────────────────────────────────
# docker-compose.prod.yml mounts a named volume over /app/frontend/themes so
# that admin-uploaded themes survive a container replacement. Docker seeds a
# named volume from the image ONLY when the volume is first created, while it
# is still empty. On every deploy after that the volume's contents shadow the
# image completely — including frontend/themes/default/, which is not user data
# at all but part of the build.
#
# That produced a genuinely confusing failure. frontend/static/ and
# frontend/templates/ are NOT under the volume, so a deploy shipped new JS and
# new admin templates while the built-in theme's templates and compiled CSS
# stayed frozen at whatever version created the volume. The mobile sidebar
# broke exactly there: new ferum-utils.js toggled a class the stale theme.css
# had no rule for, over stale nav.html that still carried an inline
# `display:none`, so the drawer could no longer be opened at all. Nothing
# errored — the two halves simply disagreed.
#
# So the image keeps a pristine copy at /app/builtin-themes, outside the mount
# point, and this script copies it over the volume on every start. Themes an
# admin uploaded live under their own slug and are never touched.
set -eu

BUILTIN_DIR="${BUILTIN_THEMES_DIR:-/app/builtin-themes}"
TARGET_DIR="${THEMES_DIR:-/app/frontend/themes}"

sync_theme() {
    slug="$1"
    src="$BUILTIN_DIR/$slug"
    dest="$TARGET_DIR/$slug"
    staging="$TARGET_DIR/.$slug.incoming"

    # Stage then swap, rather than deleting the live directory first: a copy
    # that fails halfway would otherwise leave the app with no built-in theme
    # to fall back to, which is a worse failure than a stale one. The staging
    # directory is on the same filesystem, so the final mv is a rename.
    rm -rf "$staging"
    if ! cp -a "$src" "$staging"; then
        echo "entrypoint: WARN could not stage built-in theme '$slug'; keeping the copy already in the volume" >&2
        rm -rf "$staging"
        return 0
    fi

    # `|| true` because `set -e` is on: an unguarded failure here would abort
    # the container, and refusing to boot is exactly the outcome this script is
    # not allowed to cause. A failure is reported by the mv below anyway.
    rm -rf "$dest" || true
    if ! mv "$staging" "$dest"; then
        echo "entrypoint: WARN could not install built-in theme '$slug'" >&2
        rm -rf "$staging"
        return 0
    fi

    echo "entrypoint: synced built-in theme '$slug' from the image"
}

if [ -d "$BUILTIN_DIR" ]; then
    mkdir -p "$TARGET_DIR" 2>/dev/null || true
    for path in "$BUILTIN_DIR"/*; do
        [ -d "$path" ] || continue
        # A failure here is reported and tolerated rather than fatal. The forum
        # runs on a stale built-in theme; it does not run at all if the
        # container refuses to start, and this script is not the place to make
        # that call. Same reasoning as the infrastructure fallbacks in
        # startup.rs.
        sync_theme "$(basename "$path")"
    done
else
    echo "entrypoint: WARN $BUILTIN_DIR is missing; built-in themes not synced" >&2
fi

# exec, so the server becomes PID 1 and receives SIGTERM directly. Without it
# the shell would hold PID 1, `docker stop` would never reach the process, and
# the graceful drain would be replaced by a SIGKILL ten seconds later.
exec "$@"
