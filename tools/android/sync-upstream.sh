#!/usr/bin/env bash
# Keep the Android fork in sync with upstream SolidCraft and rebuild the APK.
#
#   tools/android/sync-upstream.sh check     # what is new upstream (safe, read-only)
#   tools/android/sync-upstream.sh update    # fetch + merge upstream main into our branch
#   tools/android/sync-upstream.sh build     # build the release APK
#   tools/android/sync-upstream.sh all       # update, then build
#   tools/android/sync-upstream.sh publish   # push the branch to the fork
#
# Environment (defaults match this machine):
#   REPO_DIR, UPSTREAM_REMOTE=origin, FORK_REMOTE=fork, BRANCH=android
#   ANDROID_HOME, ANDROID_NDK_HOME, JAVA_HOME, CARGO_TARGET_DIR
set -euo pipefail

REPO_DIR=${REPO_DIR:-/Volumes/Arquivos/solidcraft-src}
UPSTREAM_REMOTE=${UPSTREAM_REMOTE:-origin}
FORK_REMOTE=${FORK_REMOTE:-fork}
BRANCH=${BRANCH:-android}
UPSTREAM_BRANCH=${UPSTREAM_BRANCH:-main}
CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-/Volumes/Arquivos/solidcraft-target}
APK="$CARGO_TARGET_DIR/release/apk/solidcraft.apk"
STAGE=${1:-check}

cd "$REPO_DIR"

say() { printf '\n\033[1m== %s\033[0m\n' "$*"; }

check() {
  say "Fetching $UPSTREAM_REMOTE/$UPSTREAM_BRANCH"
  git fetch --depth=500 "$UPSTREAM_REMOTE" "$UPSTREAM_BRANCH" --tags
  local behind
  behind=$(git rev-list --count "HEAD..$UPSTREAM_REMOTE/$UPSTREAM_BRANCH")
  echo "Branch $BRANCH is $behind commit(s) behind $UPSTREAM_REMOTE/$UPSTREAM_BRANCH."
  if [ "$behind" -gt 0 ]; then
    git log --oneline "HEAD..$UPSTREAM_REMOTE/$UPSTREAM_BRANCH" | head -20 || true
    echo
    echo "Run: $0 rebase    (Cargo.lock conflicts are resolved automatically)"
  fi
  git describe --tags --always "$UPSTREAM_REMOTE/$UPSTREAM_BRANCH" 2>/dev/null | sed 's/^/upstream: /'
}

update() {
  check
  say "Merging $UPSTREAM_REMOTE/$UPSTREAM_BRANCH into $BRANCH"
  if git merge --no-edit -m "Merge upstream $UPSTREAM_BRANCH" "$UPSTREAM_REMOTE/$UPSTREAM_BRANCH"; then
    echo "Up to date — no conflicts."
    return
  fi
  # Cargo.lock is generated: take upstream's and let cargo rewrite it on the next build.
  if git status --porcelain | grep -q '^UU Cargo.lock'; then
    echo "Cargo.lock conflicts — keeping upstream's, cargo regenerates the rest."
    git checkout --theirs Cargo.lock
    git add Cargo.lock
  fi
  if ! git diff --name-only --diff-filter=U | grep -q .; then
    git commit --no-edit -m "Merge upstream $UPSTREAM_BRANCH"
    echo "Merged."
    return
  fi
  cat <<'EOF'

Merge stopped on source conflicts: upstream changed code this fork also changed.

    git status                 # conflicted files
    # edit them, then:
    git add <files> && git commit --no-edit

Files this fork touches that upstream may also touch:
    crates/ui-egui/src/navigation.rs   (touch navigation, pinch)
    crates/ui-egui/src/viewport.rs     (touch finger list)
    Cargo.toml                          ([patch.crates-io])
Keep OUR touch blocks and take upstream's surrounding changes; when in doubt abort with
`git merge --abort` and compare `git diff $UPSTREAM_REMOTE/$UPSTREAM_BRANCH -- <file>`.
EOF
  exit 1
}

build() {
  say "Building the release APK"
  export PATH="$HOME/.cargo/bin:/usr/local/opt/openjdk@21/bin:/usr/local/bin:/usr/bin:/bin"
  export JAVA_HOME=${JAVA_HOME:-/usr/local/opt/openjdk@21}
  export ANDROID_HOME=${ANDROID_HOME:-/Volumes/Arquivos/android-sdk}
  export ANDROID_SDK_ROOT=$ANDROID_HOME
  export ANDROID_NDK_HOME=${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/27.2.12479018}
  export CC_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android28-clang"
  export AR_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-ar"
  export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$CC_aarch64_linux_android
  ( cd apps/solidcraft-android && cargo apk2 build --release --target aarch64-linux-android )
  say "APK: $APK"
  cp "$APK" "$HOME/solidcraft-android-$(date +%Y%m%d)-arm64.apk"
  echo "Copy: $HOME/solidcraft-android-$(date +%Y%m%d)-arm64.apk"
}

publish() {
  say "Pushing $BRANCH to $FORK_REMOTE"
  # Merge commits only: the public branch moves forward, never rewritten.
  git push "$FORK_REMOTE" "$BRANCH:android-latest"
}

case "$STAGE" in
  check) check ;;
  update) update ;;
  build) build ;;
  publish) publish ;;
  all) update && build && echo && echo "Install with: adb install -r $APK" ;;
  *) echo "usage: $0 {check|update|build|publish|all}" >&2; exit 2 ;;
esac
