#!/usr/bin/env bash
# Build / install Rust RDP VNC as Flatpak, generate Flathub cargo sources, or print
# Flathub PR instructions.
#
# Usage:
#   ./scripts/publish-flatpak.sh                 # build + install (user)
#   ./scripts/publish-flatpak.sh --build-only    # build .flatpak bundle
#   ./scripts/publish-flatpak.sh --generate-sources
#   ./scripts/publish-flatpak.sh --bundle        # export .flatpak file
#   ./scripts/publish-flatpak.sh --lint          # Flathub-style manifest linter (fast)
#   ./scripts/publish-flatpak.sh --preflight     # lint + offline package check before PR
#   ./scripts/publish-flatpak.sh --flathub-help   # how to open a Flathub PR
#   ./scripts/publish-flatpak.sh --uninstall
#
# Prerequisites:
#   sudo apt install flatpak flatpak-builder
#   flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo
#   flatpak install --user -y flathub \
#     org.gnome.Platform//50 org.gnome.Sdk//50 \
#     org.freedesktop.Sdk.Extension.rust-stable//25.08 \
#     org.freedesktop.Sdk.Extension.llvm20//25.08 \
#     org.flatpak.Builder
#
# Flathub (first publish) is NOT a binary upload — you open a GitHub PR.
# See --flathub-help and flatpak/README.md / flatpak/README.vi.md.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

APP_ID="io.github.manhavn.rust-rdp-vnc"
MANIFEST="flatpak/${APP_ID}.yml"
TEMPLATE="flatpak/${APP_ID}.flathub.yml.template"
BUILD_DIR="${ROOT}/.flatpak-build"
REPO_DIR="${ROOT}/.flatpak-repo"
STATE_DIR="${ROOT}/.flatpak-builder"
BUNDLE_PATH="${ROOT}/${APP_ID}.flatpak"
PREFLIGHT_DIR="${ROOT}/.flatpak-preflight"

MODE="install" # install | build-only | generate-sources | bundle | lint | preflight | flathub-help | uninstall

for arg in "$@"; do
  case "$arg" in
    --build-only) MODE="build-only" ;;
    --generate-sources) MODE="generate-sources" ;;
    --bundle) MODE="bundle" ;;
    --lint) MODE="lint" ;;
    --preflight|--check) MODE="preflight" ;;
    --flathub-help) MODE="flathub-help" ;;
    --uninstall) MODE="uninstall" ;;
    --help|-h)
      sed -n '2,35p' "$0"
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      exit 1
      ;;
  esac
done

print_flathub_help() {
  cat <<'EOF'
========== Publish to Flathub (first time) ==========

Flathub does not accept a raw binary upload. Flow:

1) Prepare a release on GitHub
   - Tag the repo, e.g. v0.1.0
   - git tag v0.1.0 && git push origin v0.1.0

2) Generate offline Cargo sources (required by Flathub)
   ./scripts/publish-flatpak.sh --generate-sources
   Or: ./scripts/publish-flathub-podman.sh  (writes flathub-out/)

3) Read submission requirements
   https://docs.flathub.org/docs/for-app-authors/submission

4) Open the submission PR — ORDER MATTERS
   Always start from upstream branch **new-pr**, then copy packaging
   files and commit. Never base a new-app PR on **master**.

   # Prefer SSH if you use an SSH key (no username/token for git):
   git clone --branch=new-pr --single-branch \
     git@github.com:flathub/flathub.git
   # or HTTPS: https://github.com/flathub/flathub.git
   cd flathub
   git checkout -b add-io.github.manhavn.rust-rdp-vnc

   # Copy package files to repo ROOT (not a subfolder)
   cp -a /path/to/flathub-out/. .
   # expect: io.github.manhavn.rust-rdp-vnc.yml, .desktop, .metainfo.xml,
   #         generated-sources.json, icon, flathub.json

   git add io.github.manhavn.rust-rdp-vnc.yml \
           io.github.manhavn.rust-rdp-vnc.desktop \
           io.github.manhavn.rust-rdp-vnc.metainfo.xml \
           io.github.manhavn.rust-rdp-vnc.png \
           generated-sources.json flathub.json
   git commit -m "Add io.github.manhavn.rust-rdp-vnc"

   # Push to YOUR fork, PR base = new-pr
   # Fork first: https://github.com/flathub/flathub/fork
   # (uncheck "Copy the master branch only")
   git remote add fork git@github.com:YOU/flathub.git   # or HTTPS
   git push -u fork HEAD
   # PR: base flathub/flathub:new-pr  ←  head YOU:add-io.github.manhavn.rust-rdp-vnc

   One-shot (SSH key — skips username/token when key works):
     GIT_AUTH=ssh OPEN_PR=1 ./scripts/publish-flathub-podman.sh

   GitHub CLI (same order — track new-pr first):
     gh auth login -h github.com -p ssh    # once
     gh repo fork --clone flathub/flathub && cd flathub
     git fetch origin new-pr && git checkout --track origin/new-pr
     git checkout -b add-io.github.manhavn.rust-rdp-vnc
     # … copy, commit, push, then:
     gh pr create --repo flathub/flathub --base new-pr \
       --title "Add io.github.manhavn.rust-rdp-vnc"

5) Manifest uses git source + offline crates (see flathub yml template):

     - type: git
       url: https://github.com/manhavn/rust-rdp-vnc.git
       tag: v0.1.0
       commit: <full commit sha of the tag>
     - generated-sources.json

6) After merge, updates go to flathub/io.github.manhavn.rust-rdp-vnc
   (not through flathub/flathub again).

Useful links:
  https://docs.flathub.org/docs/for-app-authors/submission
  https://docs.flathub.org/docs/for-app-authors/requirements
  https://github.com/flatpak/flatpak-builder-tools (cargo generator)

Local test before submitting:
  ./scripts/publish-flatpak.sh
  flatpak run io.github.manhavn.rust-rdp-vnc
=====================================================
EOF
}

ensure_flatpak() {
  if ! command -v flatpak >/dev/null 2>&1; then
    echo "flatpak not found. Install: sudo apt install flatpak flatpak-builder" >&2
    exit 1
  fi
  if ! command -v flatpak-builder >/dev/null 2>&1; then
    echo "flatpak-builder not found. Install: sudo apt install flatpak-builder" >&2
    exit 1
  fi
}

install_runtimes() {
  echo "==> Ensuring Flathub remote + SDK runtimes…"
  flatpak remote-add --if-not-exists --user flathub \
    https://dl.flathub.org/repo/flathub.flatpakrepo || true

  # Runtime versions must match the manifest (GNOME 50 → freedesktop 25.08)
  flatpak install --user -y flathub org.gnome.Platform//50 org.gnome.Sdk//50 || true
  flatpak install --user -y flathub \
    org.freedesktop.Sdk.Extension.rust-stable//25.08 \
    org.freedesktop.Sdk.Extension.llvm20//25.08 || true
}

generate_cargo_sources() {
  echo "==> Generating flatpak/generated-sources.json from Cargo.lock…"
  local gen_dir="${ROOT}/.flatpak-cargo-gen"
  local script="${gen_dir}/cargo/flatpak-cargo-generator.py"

  if [[ ! -f "$script" ]]; then
    echo "    Cloning flatpak-builder-tools (one-time)…"
    rm -rf "$gen_dir"
    git clone --depth 1 https://github.com/flatpak/flatpak-builder-tools.git "$gen_dir"
  fi

  if ! command -v python3 >/dev/null 2>&1; then
    echo "python3 required" >&2
    exit 1
  fi

  # tomli may be required depending on generator version
  python3 -m pip install --user -q toml aiohttp 2>/dev/null || true

  python3 "$script" "${ROOT}/Cargo.lock" -o "${ROOT}/flatpak/generated-sources.json"
  echo "    Wrote flatpak/generated-sources.json"
  echo "    For Flathub, add it under the rust-rdp-vnc module sources list."
  echo "    Tip: commit this file only if you want offline builds in-tree (large)."
}

build_flatpak() {
  ensure_flatpak
  install_runtimes

  if [[ ! -f "$MANIFEST" ]]; then
    echo "Missing manifest: $MANIFEST" >&2
    exit 1
  fi

  # Local manifest includes generated-sources.json — create it if missing so
  # cargo can build offline inside the flatpak-builder sandbox.
  if [[ ! -f "${ROOT}/flatpak/generated-sources.json" ]]; then
    echo "==> flatpak/generated-sources.json missing — generating (needs host network)…"
    generate_cargo_sources
  else
    echo "==> Using existing flatpak/generated-sources.json"
  fi

  echo "==> flatpak-builder (user install=${1})…"
  # shellcheck disable=SC2086
  flatpak-builder \
    --user \
    --force-clean \
    --state-dir="${STATE_DIR}" \
    --repo="${REPO_DIR}" \
    ${1:+--install} \
    "${BUILD_DIR}" \
    "${MANIFEST}"
}

ensure_flatpak_builder_app() {
  ensure_flatpak
  flatpak remote-add --if-not-exists --user flathub \
    https://dl.flathub.org/repo/flathub.flatpakrepo || true
  if ! flatpak info --user org.flatpak.Builder >/dev/null 2>&1 \
    && ! flatpak info org.flatpak.Builder >/dev/null 2>&1; then
    echo "==> Installing org.flatpak.Builder (Flathub linter + flathub-build)…"
    flatpak install --user -y flathub org.flatpak.Builder
  fi
}

# Same check Flathub CI runs on the PR (validate-manifest).
lint_manifest() {
  local mf="$1"
  ensure_flatpak_builder_app
  if [[ ! -f "$mf" ]]; then
    echo "Missing manifest: $mf" >&2
    exit 1
  fi
  echo "==> flatpak-builder-lint manifest  ($mf)"
  # Match Flathub stable exceptions set when possible.
  if flatpak run --command=flatpak-builder-lint org.flatpak.Builder \
      --exceptions --exceptions-repo stable \
      manifest "$mf"; then
    echo "    ✓ manifest lint OK"
  else
    echo
    echo "Manifest lint FAILED (same class of errors as Flathub GitHub Actions)."
    echo "Docs: https://docs.flathub.org/docs/for-app-authors/linter"
    exit 1
  fi
}

# Build the Flathub-shaped package tree (git tag + offline crates) and lint it.
prepare_flathub_package_tree() {
  local git_url git_tag git_commit out
  out="${PREFLIGHT_DIR}"
  rm -rf "$out"
  mkdir -p "$out"

  git_url="${GIT_URL:-}"
  if [[ -z "$git_url" ]]; then
    git_url="$(git -C "$ROOT" remote get-url origin 2>/dev/null || true)"
    if [[ "$git_url" =~ ^git@github.com:(.+)$ ]]; then
      git_url="https://github.com/${BASH_REMATCH[1]}"
    elif [[ "$git_url" =~ ^ssh://git@github.com/(.+)$ ]]; then
      git_url="https://github.com/${BASH_REMATCH[1]}"
    fi
  fi
  [[ -n "$git_url" ]] || git_url="https://github.com/manhavn/rust-rdp-vnc.git"

  git_tag="${GIT_TAG:-}"
  if [[ -z "$git_tag" ]]; then
    # Same bump logic as publish-flathub-podman.sh
    git -C "$ROOT" fetch origin --tags --force --prune 2>/dev/null || true
    local latest suggest
    latest="$(git -C "$ROOT" tag -l --sort=-v:refname 2>/dev/null | head -1 || true)"
    if [[ -n "$latest" && "$latest" =~ ^(.*[^0-9])([0-9]+)$ ]]; then
      suggest="$(printf "%s%d" "${BASH_REMATCH[1]}" "$((10#${BASH_REMATCH[2]} + 1))")"
    elif [[ -n "$latest" ]]; then
      suggest="${latest}.1"
    else
      suggest="v1.0.0"
    fi
    if [[ -n "$latest" ]]; then
      echo "==> Latest tag: ${latest}  →  using next: ${suggest}"
    else
      echo "==> No tags yet  →  using: ${suggest}"
    fi
    git_tag="$suggest"
  fi
  if [[ -z "$git_tag" ]]; then
    echo "No git tag found. Set GIT_TAG=vX.Y.Z (created + pushed automatically if missing)." >&2
    exit 1
  fi
  # Auto-create local tag + push to origin when the tag is missing.
  if ! git -C "$ROOT" rev-parse -q --verify "refs/tags/${git_tag}" >/dev/null; then
    echo "==> Tag ${git_tag} not found locally — creating on HEAD…"
    if [[ -n "$(git -C "$ROOT" status --porcelain 2>/dev/null || true)" ]]; then
      echo "    Note: uncommitted changes are not included in the tag (points at last commit)."
    fi
    git -C "$ROOT" tag "${git_tag}" \
      || { echo "Failed to create tag ${git_tag}" >&2; exit 1; }
    echo "    ✓ created local tag ${git_tag}"
  fi
  if ! git -C "$ROOT" ls-remote --exit-code --tags origin "refs/tags/${git_tag}" >/dev/null 2>&1; then
    echo "==> Pushing tag ${git_tag} to origin…"
    git -C "$ROOT" push origin "refs/tags/${git_tag}" \
      || { echo "Failed to push tag ${git_tag} to origin" >&2; exit 1; }
    echo "    ✓ pushed ${git_tag}"
  fi
  git_commit="$(git -C "$ROOT" rev-list -n 1 "${git_tag}")"

  if [[ ! -f "${ROOT}/flatpak/generated-sources.json" ]]; then
    echo "==> generated-sources.json missing — generating (slow)…"
    generate_cargo_sources
  fi
  [[ -f "$TEMPLATE" ]] || {
    echo "Missing template: $TEMPLATE" >&2
    exit 1
  }

  echo "==> Flathub package tree → ${out}"
  echo "    tag=${git_tag} commit=${git_commit}"
  sed \
    -e "s|__GIT_URL__|${git_url}|g" \
    -e "s|__GIT_TAG__|${git_tag}|g" \
    -e "s|__GIT_COMMIT__|${git_commit}|g" \
    "$TEMPLATE" > "${out}/${APP_ID}.yml"
  cp -a "${ROOT}/flatpak/${APP_ID}.desktop" "${out}/"
  cp -a "${ROOT}/flatpak/${APP_ID}.metainfo.xml" "${out}/"
  cp -a "${ROOT}/flatpak/generated-sources.json" "${out}/"
  cp -a "${ROOT}/desktop/assets/icon.png" "${out}/${APP_ID}.png"
  printf '%s\n' '{ "only-arches": ["x86_64"] }' > "${out}/flathub.json"
  echo "$out"
}

lint_metainfo() {
  local mi="flatpak/${APP_ID}.metainfo.xml"
  if command -v appstreamcli >/dev/null 2>&1; then
    echo "==> appstreamcli validate ${mi}"
    appstreamcli validate "$mi" || {
      echo "Metainfo validation failed (install appstream-util/appstream if needed)." >&2
      exit 1
    }
    echo "    ✓ metainfo OK"
  else
    echo "==> skip appstreamcli (not installed: sudo apt install appstream)"
  fi
}

preflight_flathub() {
  # 1) Fast: lint local + Flathub-shaped manifests (what CI fails on first)
  echo "╔══════════════════════════════════════════════════════════════╗"
  echo "║  Flathub preflight (run BEFORE opening / updating a PR)      ║"
  echo "╚══════════════════════════════════════════════════════════════╝"
  echo
  echo "Step 1/4 — Lint local development manifest (finish-args / runtime)…"
  lint_manifest "${ROOT}/${MANIFEST}"

  echo
  echo "Step 2/4 — Lint Flathub submission manifest (git + offline sources)…"
  local pkg
  pkg="$(prepare_flathub_package_tree)"
  lint_manifest "${pkg}/${APP_ID}.yml"

  echo
  echo "Step 3/4 — Validate metainfo…"
  lint_metainfo

  echo
  echo "Step 4/4 — Offline Flatpak build (same idea as Flathub builders)…"
  echo "    Working directory: ${pkg}"
  ensure_flatpak_builder_app
  install_runtimes
  (
    cd "$pkg"
    set -e
    # Prefer flathub-build (closest to Flathub CI); fall back to flatpak-builder.
    echo "==> Attempting flathub-build (Flathub-recommended)…"
    if flatpak run --command=flathub-build org.flatpak.Builder --install "${APP_ID}.yml"; then
      echo "    ✓ flathub-build OK"
    else
      echo "==> flathub-build unavailable/failed — flatpak-builder --repo=repo"
      flatpak-builder --user --force-clean --repo=repo build-dir "${APP_ID}.yml"
    fi
    if [[ -d repo ]]; then
      echo "==> Lint OSTree repo (Flathub 'repo' check)…"
      flatpak run --command=flatpak-builder-lint org.flatpak.Builder \
        --exceptions --exceptions-repo stable repo repo
      echo "    ✓ repo lint OK"
    else
      echo "    (no ./repo dir — skip repo lint; install-only builds may not export)"
    fi
  )

  echo
  echo "╔══════════════════════════════════════════════════════════════╗"
  echo "║  Preflight PASSED — safe to update the Flathub PR            ║"
  echo "╚══════════════════════════════════════════════════════════════╝"
  echo "Package tree kept at: ${PREFLIGHT_DIR}"
  echo "Copy those files to your flathub/flathub@new-pr submission branch."
  echo "Run app (if installed):  flatpak run ${APP_ID}"
}

case "$MODE" in
  flathub-help)
    print_flathub_help
    ;;
  generate-sources)
    generate_cargo_sources
    ;;
  lint)
    lint_manifest "${ROOT}/${MANIFEST}"
    if [[ -f "$TEMPLATE" ]]; then
      echo
      echo "(Also linting Flathub-shaped package if sources exist…)"
      if [[ -f "${ROOT}/flatpak/generated-sources.json" ]] \
        && git -C "$ROOT" describe --tags --abbrev=0 >/dev/null 2>&1; then
        pkg="$(prepare_flathub_package_tree)"
        lint_manifest "${pkg}/${APP_ID}.yml"
      else
        echo "    skip Flathub-tree lint: need generated-sources.json + a git tag"
        echo "    (./scripts/publish-flatpak.sh --generate-sources  and  git tag vX.Y.Z)"
      fi
    fi
    lint_metainfo
    echo
    echo "Lint done. Full offline build before PR:"
    echo "  ./scripts/publish-flatpak.sh --preflight"
    ;;
  preflight)
    preflight_flathub
    ;;
  uninstall)
    ensure_flatpak
    flatpak uninstall --user -y "${APP_ID}" || true
    echo "Uninstalled ${APP_ID} (user)."
    ;;
  build-only)
    build_flatpak 0
    echo "Build finished (not installed). Repo: ${REPO_DIR}"
    ;;
  bundle)
    build_flatpak 0
    echo "==> Exporting bundle ${BUNDLE_PATH}"
    flatpak build-bundle "${REPO_DIR}" "${BUNDLE_PATH}" "${APP_ID}"
    ls -lh "${BUNDLE_PATH}"
    echo "Install with: flatpak install --user ${BUNDLE_PATH}"
    ;;
  install)
    build_flatpak 1
    echo
    echo "Installed. Run with:"
    echo "  flatpak run ${APP_ID}"
    echo
    echo "Before Flathub PR (lint + offline build):"
    echo "  ./scripts/publish-flatpak.sh --preflight"
    echo "  ./scripts/publish-flatpak.sh --flathub-help"
    ;;
esac
