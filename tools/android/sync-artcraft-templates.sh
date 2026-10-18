#!/usr/bin/env bash
# Keep the Android bridges in apps/solidcraft-android in step with the templates they came from:
# MostafaXS1/artcraft-android (MIT). Those templates get bug fixes (SAF staging, IME, fonts) that this
# port should inherit.
#
#   tools/android/sync-artcraft-templates.sh check    # what changed upstream (read-only)
#   tools/android/sync-artcraft-templates.sh apply    # take the new templates and rebuild the Java
#
# The Java activity is generated from SafActivity.java with this app's package/class/tag, so `apply`
# regenerates it rather than copying the template verbatim.
set -euo pipefail

ART_DIR=${ART_DIR:-/Volumes/Arquivos/artcraft/repo}
APP_DIR=${APP_DIR:-/Volumes/Arquivos/solidcraft-src/apps/solidcraft-android}
PIN=${PIN:-$APP_DIR/.artcraft-templates-commit}
PKG=${PKG:-ai.storyteller.solidcraft}
CLASS=${CLASS:-SolidCraftActivity}
TAG=${TAG:-SolidCraftSAF}
STAGE=${1:-check}

say() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
[ -d "$ART_DIR/.git" ] || { echo "No artcraft clone at $ART_DIR (clone MostafaXS1/artcraft-android there)." >&2; exit 2; }

git -C "$ART_DIR" fetch origin --tags --quiet
NEW=$(git -C "$ART_DIR" rev-parse origin/main)
OLD=$(cat "$PIN" 2>/dev/null || echo "")

# Which template files this port actually uses.
FILES=(templates/android/saf_bridge.rs templates/android/ime_bridge.rs templates/android/system_fonts.rs templates/android/SafActivity.java)

say "artcraft templates: ${OLD:-desconhecido} -> $NEW"
if [ "$OLD" = "$NEW" ] && [ "$STAGE" = check ]; then
  echo "Pinned commit is current — nothing to do."
  exit 0
fi

for f in "${FILES[@]}"; do
  if [ -n "$OLD" ] && git -C "$ART_DIR" diff --quiet "$OLD" "$NEW" -- "$f"; then
    continue
  fi
  echo
  echo "--- $f"
  if [ -n "$OLD" ]; then
    git -C "$ART_DIR" diff --stat "$OLD" "$NEW" -- "$f" | tail -1
    git -C "$ART_DIR" diff "$OLD" "$NEW" -- "$f" | head -80
  else
    echo "(sem pin: a mostrar o ficheiro do upstream)"
  fi
done

[ "$STAGE" = apply ] || { echo; echo "Run: $0 apply   (then rebuild the APK)"; exit 0; }

say "Applying templates"
git -C "$ART_DIR" show "$NEW:templates/android/saf_bridge.rs"    > "$APP_DIR/src/saf_bridge.rs"
git -C "$ART_DIR" show "$NEW:templates/android/ime_bridge.rs"    > "$APP_DIR/src/ime_bridge.rs"
git -C "$ART_DIR" show "$NEW:templates/android/system_fonts.rs"  > "$APP_DIR/src/system_fonts.rs"
git -C "$ART_DIR" show "$NEW:templates/android/SafActivity.java" \
  | sed -e "s/__PACKAGE__/$PKG/g" -e "s/__CLASS__/$CLASS/g" -e "s/__TAG__/$TAG/g" \
  > "$APP_DIR/java/${PKG//.//}/$CLASS.java"
echo "Wrote saf_bridge.rs, ime_bridge.rs, system_fonts.rs and $CLASS.java."
echo "$NEW" > "$PIN"
echo "Pinned $NEW. Rebuild: cd $APP_DIR && cargo apk2 build --release --target aarch64-linux-android"
