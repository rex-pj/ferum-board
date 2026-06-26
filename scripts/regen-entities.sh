#!/usr/bin/env bash
# Regenerate Sea-ORM entities from the database schema.
#
# Closes the strong-typing loop: migrations define the schema, this script
# regenerates crates/ferum-infrastructure/src/entities/ from it, and the
# repositories (which reference entity columns) then fail to compile on any
# column/table rename.
#
#   1. Reset a throwaway "regen" database and run every migration against it.
#   2. `sea-orm-cli generate entity` over that schema, overwriting entities/.
#   3. Review with `git diff` and `cargo check`.
#
# Requires:  cargo install sea-orm-cli --version '^1.1'
# Usage:     scripts/regen-entities.sh [server_url] [regen_db_name]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
backend="$repo_root/backend"
entities_dir="$backend/crates/ferum-infrastructure/src/entities"
regen_db="${2:-ferum_board_regen}"

command -v sea-orm-cli >/dev/null 2>&1 || {
    echo "sea-orm-cli not found. Install: cargo install sea-orm-cli --version '^1.1'" >&2
    exit 1
}

server_url="${1:-}"
if [[ -z "$server_url" ]]; then
    db_url="$(grep -E '^DATABASE_URL=' "$backend/.env" | head -1 | sed 's/^DATABASE_URL=//')"
    [[ -n "$db_url" ]] || { echo "DATABASE_URL not found in $backend/.env" >&2; exit 1; }
    server_url="${db_url%/*}"
fi

regen_url="$server_url/$regen_db"
echo "==> Regen database: $regen_url"

cd "$backend"
export DATABASE_URL="$regen_url"

echo "==> migration fresh"
cargo run --quiet --package migration -- fresh

echo "==> sea-orm-cli generate entity -> $entities_dir"
sea-orm-cli generate entity \
    --database-url "$regen_url" \
    --output-dir "$entities_dir" \
    --with-serde both \
    --date-time-crate chrono

echo
echo "Done. Review and verify:"
echo "  git -C '$repo_root' diff -- backend/crates/ferum-infrastructure/src/entities/"
echo "  cargo check --manifest-path '$backend/Cargo.toml'"
