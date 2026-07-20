#!/usr/bin/env bash
# Interactive one-shot helper: prepare (and optionally open) a Flathub submission
# using Podman for reproducible tooling.
#
# IMPORTANT — Flathub is NOT a username/password binary upload store.
#   • First publish  = GitHub PR to flathub/flathub (branch new-pr)
#   • Later updates  = PR to flathub/io.github.manhavn.rust-rdp-vnc
# This script can: generate cargo sources, build a Flathub package tree,
# optionally push a branch and open the PR (SSH key or HTTPS token).
#
# Usage:
#   ./scripts/publish-flathub-podman.sh
#   ./scripts/publish-flathub-podman.sh --non-interactive   # use env vars only
#   GIT_AUTH=ssh OPEN_PR=1 ./scripts/publish-flathub-podman.sh   # SSH key, no token
#   GIT_AUTH=https GH_TOKEN=... GH_USER=you OPEN_PR=1 ./scripts/publish-flathub-podman.sh
#
# First interactive prompt is GitHub transport (ssh | https).
#
# Env (optional, skips prompts when set):
#   GIT_AUTH=ssh|https       GitHub transport for clone/push/PR (default: ssh)
#                            ssh → git@github.com:…  (no username/token if key works)
#                            https → needs GH_USER + GH_TOKEN when OPEN_PR=1
#   GH_TOKEN / GITHUB_TOKEN  GitHub PAT (HTTPS only; repo + workflow recommended)
#   GH_USER                  GitHub username (auto-detected via gh / origin when possible)
#   GIT_URL                  Upstream project URL for the Flatpak manifest (HTTPS preferred)
#   GIT_TAG                  Release tag (e.g. v0.1.0); created + pushed if missing
#   WORK_DIR                 Where to write the flathub package (default: ./flathub-out)
#   SKIP_BUILD=1             Skip podman flatpak-builder smoke test
#   OPEN_PR=1|0              Open GitHub PR automatically (default: ask)
#   MODE=first|update        first = flathub/flathub new-pr; update = app repo
#   FLATHUB_VIDEO_URL        Public demo video URL for the submission PR body
#                            (GitHub user-attachments or raw .webm URL)
#   TOOLS_IMAGE              Podman tools image ref
#                            (default: build.dev/flathub-rust-rdp-vnc:v1.0.0)
#   TOOLS_IMAGE_TAR          Optional path to image .tar under scripts/
#                            (default: scripts/<image-ref with specials→.>.tar)
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

APP_ID="io.github.manhavn.rust-rdp-vnc"
TEMPLATE="${ROOT}/flatpak/${APP_ID}.flathub.yml.template"
NON_INTERACTIVE=0

for arg in "$@"; do
  case "$arg" in
    --non-interactive) NON_INTERACTIVE=1 ;;
    --help|-h)
      sed -n '2,35p' "$0"
      exit 0
      ;;
    *)
      echo "Unknown arg: $arg" >&2
      exit 1
      ;;
  esac
done

# ── helpers ──────────────────────────────────────────────────────────────────

die() { echo "ERROR: $*" >&2; exit 1; }
info() { echo "==> $*"; }
ok() { echo "    ✓ $*"; }

prompt() {
  # prompt VAR "Question" "default"
  local var="$1" q="$2" def="${3:-}"
  local cur="${!var:-}"
  if [[ -n "$cur" ]]; then
    return 0
  fi
  if [[ "$NON_INTERACTIVE" -eq 1 ]]; then
    if [[ -n "$def" ]]; then
      printf -v "$var" '%s' "$def"
      return 0
    fi
    die "Missing required env: $var"
  fi
  local ans
  if [[ -n "$def" ]]; then
    read -r -p "$q [$def]: " ans || true
    ans="${ans:-$def}"
  else
    read -r -p "$q: " ans || true
  fi
  printf -v "$var" '%s' "$ans"
}

prompt_secret() {
  local var="$1" q="$2"
  local cur="${!var:-}"
  if [[ -n "$cur" ]]; then
    return 0
  fi
  if [[ "$NON_INTERACTIVE" -eq 1 ]]; then
    die "Missing required secret env: $var"
  fi
  local ans
  read -r -s -p "$q: " ans || true
  echo
  printf -v "$var" '%s' "$ans"
}

prompt_yesno() {
  # sets VAR to 1 or 0
  local var="$1" q="$2" def="${3:-y}"
  local cur="${!var:-}"
  if [[ -n "$cur" ]]; then
    return 0
  fi
  if [[ "$NON_INTERACTIVE" -eq 1 ]]; then
    if [[ "$def" =~ ^[Yy] ]]; then printf -v "$var" '1'; else printf -v "$var" '0'; fi
    return 0
  fi
  local ans
  read -r -p "$q [y/n] (default $def): " ans || true
  ans="${ans:-$def}"
  if [[ "$ans" =~ ^[Yy] ]]; then printf -v "$var" '1'; else printf -v "$var" '0'; fi
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "Missing command: $1"
}

default_git_url() {
  local u
  u="$(git -C "$ROOT" remote get-url origin 2>/dev/null || true)"
  if [[ -z "$u" ]]; then
    echo "https://github.com/manhavn/rust-rdp-vnc.git"
    return
  fi
  # Manifest sources should stay HTTPS (Flathub builders pull over https).
  if [[ "$u" =~ ^git@github.com:(.+)$ ]]; then
    echo "https://github.com/${BASH_REMATCH[1]}"
  elif [[ "$u" =~ ^ssh://git@github.com/(.+)$ ]]; then
    echo "https://github.com/${BASH_REMATCH[1]}"
  else
    echo "$u"
  fi
}

# owner/repo (no .git) → git@github.com:owner/repo.git
github_ssh_url() {
  local path="${1%.git}"
  path="${path#/}"
  echo "git@github.com:${path}.git"
}

# owner/repo (no .git) → https://github.com/owner/repo.git
github_https_url() {
  local path="${1%.git}"
  path="${path#/}"
  echo "https://github.com/${path}.git"
}

# Batch-mode probe: key works without interactive password.
ssh_github_ok() {
  local out
  out="$(ssh -o BatchMode=yes -o ConnectTimeout=8 -o StrictHostKeyChecking=accept-new \
    -T git@github.com 2>&1 || true)"
  [[ "$out" == *"successfully authenticated"* || "$out" == *"Hi "* ]]
}

# Fill GH_USER from gh CLI or origin remote when possible.
detect_gh_user() {
  if [[ -n "${GH_USER:-}" ]]; then
    return 0
  fi
  if command -v gh >/dev/null 2>&1; then
    GH_USER="$(gh api user -q .login 2>/dev/null || true)"
  fi
  if [[ -z "${GH_USER:-}" ]]; then
    local u
    u="$(git -C "$ROOT" remote get-url origin 2>/dev/null || true)"
    if [[ "$u" =~ git@github\.com:([^/]+)/ ]]; then
      GH_USER="${BASH_REMATCH[1]}"
    elif [[ "$u" =~ github\.com[/:]([^/]+)/ ]]; then
      GH_USER="${BASH_REMATCH[1]}"
    fi
  fi
  export GH_USER
}

# URL used for clone/push of a GitHub repo path (owner/name).
github_repo_url() {
  local path="$1"
  if [[ "${GIT_AUTH}" == "ssh" ]]; then
    github_ssh_url "$path"
  else
    github_https_url "$path"
  fi
}

# Push URL for the user's fork (token embedded only for HTTPS + token).
github_fork_push_url() {
  local path="$1" # e.g. user/flathub
  if [[ "${GIT_AUTH}" == "ssh" ]]; then
    github_ssh_url "$path"
  elif [[ -n "${GH_TOKEN:-}" ]]; then
    echo "https://x-access-token:${GH_TOKEN}@github.com/${path%.git}.git"
  else
    github_https_url "$path"
  fi
}

# ── banner ───────────────────────────────────────────────────────────────────

cat <<'EOF'
╔══════════════════════════════════════════════════════════════╗
║  Rust RDP VNC → Flathub helper (Podman)                      ║
╠══════════════════════════════════════════════════════════════╣
║  Flathub does NOT accept store login + binary upload.        ║
║  This wizard prepares sources + package and can open a       ║
║  GitHub PR via SSH key (git@github.com) or HTTPS + token.    ║
║  Reviewers still must approve.                               ║
╚══════════════════════════════════════════════════════════════╝
EOF

need_cmd podman
need_cmd git

# ── collect inputs ───────────────────────────────────────────────────────────

GH_TOKEN="${GH_TOKEN:-${GITHUB_TOKEN:-}}"
GH_USER="${GH_USER:-}"
GIT_URL="${GIT_URL:-}"
GIT_TAG="${GIT_TAG:-}"
WORK_DIR="${WORK_DIR:-}"
SKIP_BUILD="${SKIP_BUILD:-}"
OPEN_PR="${OPEN_PR:-}"
MODE="${MODE:-}"
GIT_AUTH="${GIT_AUTH:-}"

# First question: how we talk to GitHub (tag push, clone flathub, open PR).
prompt GIT_AUTH "GitHub transport: ssh (git@github.com) or https" "ssh"
case "${GIT_AUTH}" in
  ssh|https) ;;
  *) die "GIT_AUTH must be 'ssh' or 'https' (got: ${GIT_AUTH})" ;;
esac
export GIT_AUTH

if [[ "${GIT_AUTH}" == "ssh" ]]; then
  info "Using SSH (git@github.com:) for git push/clone/PR — no token required"
  need_cmd ssh
  if ! ssh_github_ok; then
    die "SSH auth to git@github.com failed (BatchMode).
  Fix: ssh-add your key, or test:  ssh -T git@github.com
  Or re-run and choose https + a token."
  fi
  ok "SSH to GitHub works"
  detect_gh_user
  if [[ -n "${GH_USER:-}" ]]; then
    ok "GitHub user: ${GH_USER}"
  fi
  if command -v gh >/dev/null 2>&1 && ! gh auth status >/dev/null 2>&1; then
    info "Tip: for automatic PR open, run once:  gh auth login -h github.com -p ssh"
  fi
else
  info "Using HTTPS for git push/clone/PR (token needed if you open a PR or push tags to a private remote)"
  detect_gh_user
  if [[ -n "${GH_USER:-}" ]]; then
    ok "GitHub user: ${GH_USER}"
  fi
fi

# Flatpak manifest source should stay HTTPS (Flathub builders pull over https).
prompt GIT_URL "Project URL for Flatpak manifest (HTTPS preferred)" "$(default_git_url)"

# Latest tag → suggest next by bumping the last numeric component (v1.0.0 → v1.0.1).
latest_git_tag() {
  # Version-sort prefers semver-ish names; fall back to creatordate if empty.
  local t
  t="$(git -C "$ROOT" tag -l --sort=-v:refname 2>/dev/null | head -1 || true)"
  if [[ -z "$t" ]]; then
    t="$(git -C "$ROOT" tag -l --sort=-creatordate 2>/dev/null | head -1 || true)"
  fi
  printf '%s' "$t"
}

suggest_next_tag() {
  local latest="$1"
  if [[ -z "$latest" ]]; then
    printf '%s' "v1.0.0"
    return
  fi
  # Bump trailing integer: v1.0.0 → v1.0.1, release-2 → release-3, 9 → 10
  if [[ "$latest" =~ ^(.*[^0-9])([0-9]+)$ ]]; then
    local prefix="${BASH_REMATCH[1]}"
    local num="${BASH_REMATCH[2]}"
    # Preserve zero-padding width when present (01 → 02).
    local width=${#num}
    local next=$((10#$num + 1))
    printf "%s%0*d" "$prefix" "$width" "$next"
  elif [[ "$latest" =~ ^([0-9]+)$ ]]; then
    printf '%s' "$((10#$latest + 1))"
  else
    # No trailing digits — append .1
    printf '%s.1' "$latest"
  fi
}

info "Fetching tags from origin (for latest release)…"
git -C "$ROOT" fetch origin --tags --force --prune 2>/dev/null \
  || info "Could not fetch origin tags (using local tags only)"

LATEST_TAG="$(latest_git_tag)"
SUGGEST_TAG="$(suggest_next_tag "${LATEST_TAG}")"
if [[ -n "${LATEST_TAG}" ]]; then
  info "Latest tag: ${LATEST_TAG}  →  suggest next: ${SUGGEST_TAG}"
else
  info "No existing tags  →  suggest: ${SUGGEST_TAG}"
fi

# Env GIT_TAG wins; otherwise prompt with suggested bump.
if [[ -z "${GIT_TAG}" ]]; then
  prompt GIT_TAG "Release git tag (created + pushed automatically if missing)" "${SUGGEST_TAG}"
else
  info "Using GIT_TAG from environment: ${GIT_TAG}"
fi

prompt WORK_DIR "Output directory for Flathub package tree" "${ROOT}/flathub-out"
prompt MODE "Submit mode: first (new app PR) or update (app repo PR)" "first"

if [[ "$MODE" != "first" && "$MODE" != "update" ]]; then
  die "MODE must be 'first' or 'update'"
fi

prompt_yesno DO_GEN "Generate flatpak/generated-sources.json now?" "y"
if [[ -z "${SKIP_BUILD}" ]]; then
  prompt_yesno SKIP_BUILD "Skip Podman flatpak-builder smoke test? (faster)" "y"
fi
prompt_yesno OPEN_PR "Open GitHub PR automatically?" "n"

# Credentials needed for OPEN_PR (and optional HTTPS authenticated ops).
if [[ "${OPEN_PR}" == "1" ]]; then
  if [[ "${GIT_AUTH}" == "ssh" ]]; then
    if [[ -z "${GH_USER:-}" ]]; then
      prompt GH_USER "GitHub username (fork owner / PR head)"
    fi
  else
    prompt GH_USER "GitHub username"
    prompt_secret GH_TOKEN "GitHub Personal Access Token (repo scope)"
    export GH_TOKEN GITHUB_TOKEN="$GH_TOKEN"
  fi
  export GH_USER
fi

# ── ensure release tag (create + push if missing) ────────────────────────────

# Create local tag on HEAD if absent, then push to origin when remote lacks it.
ensure_release_tag() {
  local tag="$1"
  local head remote_has
  [[ -n "$tag" ]] || die "Empty GIT_TAG"

  head="$(git -C "$ROOT" rev-parse HEAD)"
  info "Resolving release tag ${tag}…"

  if ! git -C "$ROOT" rev-parse -q --verify "refs/tags/${tag}" >/dev/null; then
    if [[ -n "$(git -C "$ROOT" status --porcelain 2>/dev/null || true)" ]]; then
      info "Note: working tree has uncommitted changes — tag points at last commit only (${head:0:12})."
    fi
    info "Tag ${tag} not found locally — creating on HEAD (${head:0:12})…"
    git -C "$ROOT" tag "${tag}" \
      || die "Failed to create local tag ${tag}"
    ok "created local tag ${tag}"
  else
    ok "local tag ${tag} exists → $(git -C "$ROOT" rev-list -n 1 "${tag}" | cut -c1-12)"
  fi

  # Push to origin if the tag is not on the remote yet.
  remote_has=0
  if git -C "$ROOT" ls-remote --exit-code --tags origin "refs/tags/${tag}" >/dev/null 2>&1; then
    remote_has=1
  fi
  if [[ "$remote_has" -eq 0 ]]; then
    info "Pushing tag ${tag} to origin…"
    git -C "$ROOT" push origin "refs/tags/${tag}" \
      || die "Failed to push tag ${tag} to origin (check git remote / auth)"
    ok "pushed ${tag} to origin"
  else
    ok "origin already has tag ${tag}"
  fi
}

ensure_release_tag "${GIT_TAG}"
GIT_COMMIT="$(git -C "$ROOT" rev-list -n 1 "${GIT_TAG}")"
ok "commit ${GIT_COMMIT}"

# ── Podman image with tools ──────────────────────────────────────────────────
# Bake tools into a named image once, save as .tar under scripts/ for reload.
# Default ref: build.dev/flathub-rust-rdp-vnc:v1.0.0
# Tar name: special chars → '.' ; keep '-'  →  scripts/build.dev.flathub-rust-rdp-vnc.v1.0.0.tar

TOOLS_IMAGE="${TOOLS_IMAGE:-build.dev/flathub-rust-rdp-vnc:v1.0.0}"
CACHE_VOL="rust-rdp-vnc-flathub-cache"
SCRIPTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# build.dev/flathub-rust-rdp-vnc:v1.0.0 → build.dev.flathub-rust-rdp-vnc.v1.0.0.tar
image_ref_to_tar_basename() {
  local ref="$1"
  # Keep letters, digits, hyphen, and dots; every other char becomes '.'
  local base
  base="$(printf '%s' "$ref" | sed 's/[^A-Za-z0-9.-]/./g')"
  # Collapse accidental runs of dots from consecutive specials (e.g. "://")
  base="$(printf '%s' "$base" | sed 's/\.\+/\./g; s/^\.//; s/\.$//')"
  printf '%s.tar' "$base"
}

TOOLS_IMAGE_TAR="${TOOLS_IMAGE_TAR:-${SCRIPTS_DIR}/$(image_ref_to_tar_basename "${TOOLS_IMAGE}")}"

save_tools_image_tar() {
  info "Saving Podman image → ${TOOLS_IMAGE_TAR}"
  mkdir -p "$(dirname "${TOOLS_IMAGE_TAR}")"
  # Atomic-ish write: save to .partial then rename
  local partial="${TOOLS_IMAGE_TAR}.partial"
  rm -f "${partial}"
  podman save -o "${partial}" "${TOOLS_IMAGE}" \
    || die "podman save failed for ${TOOLS_IMAGE}"
  mv -f "${partial}" "${TOOLS_IMAGE_TAR}"
  ok "saved $(du -h "${TOOLS_IMAGE_TAR}" | awk '{print $1}')  ${TOOLS_IMAGE_TAR}"
}

load_tools_image_tar() {
  [[ -f "${TOOLS_IMAGE_TAR}" ]] || return 1
  info "Loading Podman image from tar (skip rebuild)…"
  info "  ${TOOLS_IMAGE_TAR}"
  podman load -i "${TOOLS_IMAGE_TAR}" \
    || die "podman load failed: ${TOOLS_IMAGE_TAR}"
  if ! podman image exists "${TOOLS_IMAGE}" 2>/dev/null; then
    # Older tars / retag: try to tag the most recently loaded image id
    die "Loaded tar but image ref ${TOOLS_IMAGE} is missing.
  Rebuild once: rm -f '${TOOLS_IMAGE_TAR}' && re-run this script."
  fi
  ok "loaded ${TOOLS_IMAGE}"
}

build_tools_image() {
  info "Building tools image ${TOOLS_IMAGE} (one-time, ~1–2 min)…"
  podman build -t "${TOOLS_IMAGE}" -f - <<'EOF'
FROM docker.io/library/ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update -qq \
 && apt-get install -y --no-install-recommends \
      git python3 python3-pip python3-venv python3-full \
      curl ca-certificates build-essential pkg-config \
 && rm -rf /var/lib/apt/lists/* \
 && python3 --version
EOF
  ok "tools image built: ${TOOLS_IMAGE}"
}

info "Ensuring Podman tools image (${TOOLS_IMAGE})…"
info "Image archive: ${TOOLS_IMAGE_TAR}"

if podman image exists "${TOOLS_IMAGE}" 2>/dev/null; then
  ok "tools image already present in Podman"
  # Keep a portable cache under scripts/ for other machines / clean podman stores
  if [[ ! -f "${TOOLS_IMAGE_TAR}" ]]; then
    save_tools_image_tar
  else
    ok "tar cache already exists"
  fi
elif [[ -f "${TOOLS_IMAGE_TAR}" ]]; then
  load_tools_image_tar
else
  build_tools_image
  save_tools_image_tar
fi

info "Ensuring Podman cache volume ${CACHE_VOL}…"
podman volume exists "${CACHE_VOL}" 2>/dev/null || podman volume create "${CACHE_VOL}" >/dev/null

run_in_podman() {
  podman run --rm -i \
    -v "${ROOT}:/src:Z" \
    -v "${CACHE_VOL}:/cache:Z" \
    -w /src \
    -e DEBIAN_FRONTEND=noninteractive \
    "${TOOLS_IMAGE}" \
    bash -lc "$*"
}

# Shared setup: repair venv if broken, ensure flatpak-builder-tools clone.
PODMAN_SETUP='
  set -euo pipefail
  export PATH="/cache/bin:/cache/venv/bin:${PATH:-/usr/bin}"

  ensure_python() {
    # Generator deps: tomlkit (required by current flatpak-cargo-generator), aiohttp
    if /cache/venv/bin/python3 -c "import tomlkit, aiohttp" 2>/dev/null; then
      return 0
    fi
    echo "Creating / repairing Python venv on cache volume..."
    rm -rf /cache/venv
    python3 -m venv /cache/venv
    /cache/venv/bin/python3 -m pip install -U pip -q
    /cache/venv/bin/python3 -m pip install -q \
      "tomlkit>=0.12" "aiohttp>=3.8" "toml>=0.10"
    mkdir -p /cache/bin
    ln -sfn /cache/venv/bin/python3 /cache/bin/python3
    ln -sfn /cache/venv/bin/python3 /cache/bin/python
    /cache/venv/bin/python3 -c "import tomlkit, aiohttp; print(\"python-ok\")"
  }

  ensure_tools_repo() {
    if [[ ! -f /cache/flatpak-builder-tools/cargo/flatpak-cargo-generator.py ]]; then
      rm -rf /cache/flatpak-builder-tools
      git clone --depth 1 https://github.com/flatpak/flatpak-builder-tools.git \
        /cache/flatpak-builder-tools
    fi
  }

  ensure_python
  ensure_tools_repo
'

info "Preparing Python + flatpak-builder-tools inside Podman…"
run_in_podman "${PODMAN_SETUP}
  echo tools-ready
"
ok "Podman tooling ready"

# ── generate cargo sources ───────────────────────────────────────────────────

if [[ "${DO_GEN}" == "1" ]]; then
  info "Generating flatpak/generated-sources.json (can take several minutes)…"
  run_in_podman "${PODMAN_SETUP}
    /cache/venv/bin/python3 \
      /cache/flatpak-builder-tools/cargo/flatpak-cargo-generator.py \
      /src/Cargo.lock \
      -o /src/flatpak/generated-sources.json
    ls -lh /src/flatpak/generated-sources.json
  "
  ok "generated-sources.json"
else
  [[ -f "${ROOT}/flatpak/generated-sources.json" ]] \
    || die "flatpak/generated-sources.json missing — re-run with generate = y"
  ok "using existing generated-sources.json"
fi

# ── assemble Flathub package (TOPLEVEL only — bot: "Files not in toplevel") ─

PKG_DIR="${WORK_DIR%/}"
# Never wipe/write packaging helpers into the git repo root by accident.
if [[ -z "${PKG_DIR}" || "${PKG_DIR}" == "${ROOT}" || "${PKG_DIR}" == "." || "${PKG_DIR}" == "./" ]]; then
  die "WORK_DIR must not be the repo root (got: '${WORK_DIR:-}'). Use e.g. ${ROOT}/flathub-out"
fi
info "Writing Flathub package (toplevel layout) → ${PKG_DIR}"
rm -rf "${PKG_DIR}"
mkdir -p "${PKG_DIR}"

[[ -f "$TEMPLATE" ]] || die "Missing template: $TEMPLATE"

sed \
  -e "s|__GIT_URL__|${GIT_URL}|g" \
  -e "s|__GIT_TAG__|${GIT_TAG}|g" \
  -e "s|__GIT_COMMIT__|${GIT_COMMIT}|g" \
  "$TEMPLATE" > "${PKG_DIR}/${APP_ID}.yml"

cp -a "${ROOT}/flatpak/${APP_ID}.desktop" "${PKG_DIR}/"
cp -a "${ROOT}/flatpak/${APP_ID}.metainfo.xml" "${PKG_DIR}/"
cp -a "${ROOT}/flatpak/generated-sources.json" "${PKG_DIR}/"
cp -a "${ROOT}/desktop/assets/icon.png" "${PKG_DIR}/${APP_ID}.png"
printf '%s\n' '{ "only-arches": ["x86_64"] }' > "${PKG_DIR}/flathub.json"

# Official Flathub first-submission PR description (base MUST be new-pr).
# Optional: FLATHUB_VIDEO_URL=https://github.com/user-attachments/assets/...
UPSTREAM_WEB="${GIT_URL%.git}"
UPSTREAM_WEB="${UPSTREAM_WEB%/}"
if [[ "$UPSTREAM_WEB" =~ ^git@github.com:(.+)$ ]]; then
  UPSTREAM_WEB="https://github.com/${BASH_REMATCH[1]}"
elif [[ "$UPSTREAM_WEB" =~ ^ssh://git@github.com/(.+)$ ]]; then
  UPSTREAM_WEB="https://github.com/${BASH_REMATCH[1]}"
fi
SCREENSHOT_URL_1="${FLATHUB_SCREENSHOT_URL:-https://raw.githubusercontent.com/manhavn/rust-rdp-vnc/main/desktop/assets/screenshots/connection.png}"
SCREENSHOT_URL_2="${FLATHUB_SCREENSHOT_URL_2:-https://raw.githubusercontent.com/manhavn/rust-rdp-vnc/main/desktop/assets/screenshots/session.png}"
# Back-compat for older docs that expect a single SCREENSHOT_URL
SCREENSHOT_URL="${SCREENSHOT_URL_1}"
FLATHUB_VIDEO_URL="${FLATHUB_VIDEO_URL:-}"

write_flathub_pr_body() {
  local out="$1"
  local video_line
  if [[ -n "${FLATHUB_VIDEO_URL}" ]]; then
    video_line="${FLATHUB_VIDEO_URL}"
  else
    video_line="**(attach a short demo video of the Flatpak running on Linux — drag-drop into the PR description)**"
  fi
  cat > "${out}" <<EOF
<!-- ⚠️⚠️  Submission pull request MUST be made against the \`new-pr\` **base branch** ⚠️⚠️  -->

### Please confirm your submission meets all the criteria

- [x] Please describe the application briefly. **Rust RDP VNC is a Linux remote desktop client (RDP via IronRDP + VNC) with a native desktop UI.**
- [x] Please attach a video showcasing the application on Linux using the Flatpak. ${video_line}
- [x] The Flatpak ID follows all the rules listed in the Application ID requirements.
- [x] I have read and followed all the Submission requirements and the Submission guide and I agree to them.
- [x] I am an author/developer/upstream contributor to the project. **Link:** ${UPSTREAM_WEB}

Upstream: ${UPSTREAM_WEB}
Tag: ${GIT_TAG} (${GIT_COMMIT})
Screenshots:
- ${SCREENSHOT_URL_1}
- ${SCREENSHOT_URL_2}
App ID: \`${APP_ID}\`

<!-- ⚠️⚠️  Please DO NOT change anything below this line ⚠️⚠️  -->

[appid]: https://docs.flathub.org/docs/for-app-authors/requirements#application-id
[reqs]: https://docs.flathub.org/docs/for-app-authors/requirements
[reqs2]: https://docs.flathub.org/docs/for-app-authors/submission
EOF
}

write_flathub_pr_body "${PKG_DIR}/PR-BODY.md"

# Title depends on submit mode (first vs update).
if [[ "${MODE}" == "update" ]]; then
  PR_TITLE="Update ${APP_ID} to ${GIT_TAG}"
else
  PR_TITLE="Add ${APP_ID}"
fi
printf '%s\n' "${PR_TITLE}" > "${PKG_DIR}/PR-TITLE.txt"

# Pretty printer so the user can copy/paste if gh pr create fails later.
print_pr_copy_block() {
  local title="${1:-${PR_TITLE}}"
  local body_file="${2:-${PKG_DIR}/PR-BODY.md}"
  echo
  echo "╔══════════════════════════════════════════════════════════════╗"
  echo "║  PR title + description (copy to recreate PR if needed)       ║"
  echo "╚══════════════════════════════════════════════════════════════╝"
  echo
  echo "Base branch:  new-pr   (flathub/flathub — first submit only)"
  echo "Repo:         flathub/flathub   (or flathub/${APP_ID} for updates)"
  echo
  echo "──────── PR title (copy below) ────────────────────────────────"
  echo "${title}"
  echo "──────── end title ────────────────────────────────────────────"
  echo
  echo "──────── PR description (copy below) ──────────────────────────"
  if [[ -f "${body_file}" ]]; then
    cat "${body_file}"
  else
    echo "(missing body file: ${body_file})"
  fi
  echo
  echo "──────── end description ──────────────────────────────────────"
  echo
  echo "Saved for later:"
  echo "  title: ${PKG_DIR}/PR-TITLE.txt"
  echo "  body:  ${body_file}"
  echo
}

cat > "${PKG_DIR}/README-SUBMIT.md" <<EOF
# Flathub package for ${APP_ID}

- Tag: ${GIT_TAG}
- Commit: ${GIT_COMMIT}
- Upstream: ${UPSTREAM_WEB}
- App ID: \`${APP_ID}\`
- Screenshots:
  - ${SCREENSHOT_URL_1}
  - ${SCREENSHOT_URL_2}

## ⚠️ PR base branch

Submission PR **MUST** target **\`flathub/flathub\` base = \`new-pr\`** (never \`master\`).

## PR description

Copy **[\`PR-BODY.md\`](./PR-BODY.md)** into the GitHub PR body (or use
\`OPEN_PR=1\` which passes it to \`gh pr create\`).

Set a video URL before packaging if you already uploaded one:

\`\`\`bash
export FLATHUB_VIDEO_URL='https://github.com/user-attachments/assets/…'
./scripts/publish-flathub-podman.sh
\`\`\`

Or attach \`desktop/assets/screenshots/demo.webm\` by drag-and-drop on the PR.

## First submission (required order)

Start from **upstream** \`new-pr\`, then copy files and commit. Do **not**
base the PR on \`master\`.

### SSH (recommended if you use an SSH key — no token)

\`\`\`bash
# 1) Clone flathub/flathub @ new-pr via SSH
git clone --branch=new-pr --single-branch \\
  git@github.com:flathub/flathub.git
cd flathub

# 2) Submission branch from new-pr
git checkout -b add-${APP_ID}

# 3) Copy packaging files to repo ROOT (not a subfolder)
cp -a ${PKG_DIR}/. .
rm -f README-SUBMIT.md PR-BODY.md PR-TITLE.txt

# 4) Commit
git add ${APP_ID}.yml ${APP_ID}.desktop ${APP_ID}.metainfo.xml \\
  ${APP_ID}.png generated-sources.json flathub.json
git commit -m "Add ${APP_ID} (${GIT_TAG})"

# 5) Push to YOUR fork (SSH) and open PR (base = new-pr)
#    Fork first: https://github.com/flathub/flathub/fork
#    (uncheck "Copy the master branch only")
git remote add fork git@github.com:YOU/flathub.git
git push -u fork HEAD
gh pr create --repo flathub/flathub --base new-pr \\
  --title "Add ${APP_ID}" \\
  --body-file ${PKG_DIR}/PR-BODY.md
# (gh: once  →  gh auth login -h github.com -p ssh)
\`\`\`

### HTTPS + token

\`\`\`bash
git clone --branch=new-pr --single-branch \\
  https://github.com/flathub/flathub.git
cd flathub
git checkout -b add-${APP_ID}
cp -a ${PKG_DIR}/. . && rm -f README-SUBMIT.md PR-BODY.md PR-TITLE.txt
git add ${APP_ID}.yml ${APP_ID}.desktop ${APP_ID}.metainfo.xml \\
  ${APP_ID}.png generated-sources.json flathub.json
git commit -m "Add ${APP_ID} (${GIT_TAG})"
git remote add fork https://github.com/YOU/flathub.git
git push -u fork HEAD
gh pr create --repo flathub/flathub --base new-pr \\
  --title "Add ${APP_ID}" \\
  --body-file ${PKG_DIR}/PR-BODY.md
\`\`\`

Or one-shot with this repo's script:

\`\`\`bash
# SSH key only (no username/token prompts when key + gh work):
FLATHUB_VIDEO_URL='https://github.com/user-attachments/assets/…' \\
  GIT_AUTH=ssh OPEN_PR=1 ./scripts/publish-flathub-podman.sh

# HTTPS:
GIT_AUTH=https GH_USER=you GH_TOKEN=ghp_… OPEN_PR=1 ./scripts/publish-flathub-podman.sh
\`\`\`

## Update (after the app is on Flathub)
Clone flathub/${APP_ID} and replace packaging files; open a PR there.
EOF

ok "package tree ready (toplevel)"
ls -la "${PKG_DIR}"

# Always show PR text early (package is ready even if PR open fails later).
print_pr_copy_block "${PR_TITLE}" "${PKG_DIR}/PR-BODY.md"

# ── optional smoke build ─────────────────────────────────────────────────────

if [[ "${SKIP_BUILD}" != "1" ]]; then
  info "Smoke-testing with flatpak-builder in Podman (long)…"
  run_in_podman '
    set -e
    apt-get update -qq
    apt-get install -y -qq flatpak flatpak-builder ostree elfutils 2>/dev/null | tail -3
    flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo || true
    # Runtimes are huge; only attempt if user wants full build
    echo "NOTE: Full runtime install inside container is multi-GB."
    echo "Prefer host: ./scripts/publish-flatpak.sh for local install tests."
  ' || true
  ok "skipped full runtime install in container (use host script to test run)"
fi

# ── optional GitHub PR ───────────────────────────────────────────────────────

if [[ "${OPEN_PR}" == "1" ]]; then
  if ! command -v gh >/dev/null 2>&1; then
    die "Install GitHub CLI: https://cli.github.com/  (sudo apt install gh) then re-run with OPEN_PR=1"
  fi

  # HTTPS+token: feed gh. SSH: use existing gh session (or keyring), never invent a token.
  if [[ "${GIT_AUTH}" == "https" ]]; then
    [[ -n "${GH_TOKEN:-}" ]] || die "GH_TOKEN required for GIT_AUTH=https"
    echo "${GH_TOKEN}" | gh auth login --with-token 2>/dev/null || true
  elif [[ -n "${GH_TOKEN:-}" ]]; then
    # Optional: token only for gh API while git stays on SSH
    echo "${GH_TOKEN}" | gh auth login --with-token 2>/dev/null || true
  fi

  BRANCH="rust-rdp-vnc-${GIT_TAG//\//-}-$(date +%Y%m%d%H%M)"
  TMP_GH="$(mktemp -d)"
  cleanup() { rm -rf "${TMP_GH}"; }
  trap cleanup EXIT

  if [[ "$MODE" == "first" ]]; then
    # Official flow (https://docs.flathub.org/docs/for-app-authors/submission):
    #   1) Start from flathub/flathub @ branch new-pr (upstream, not master)
    #   2) Create a submission branch, copy packaging files to repo ROOT
    #   3) Commit, push to YOUR fork, open PR with base = new-pr
    info "Preparing first-app PR against flathub/flathub (base: new-pr)…"
    UPSTREAM_URL="$(github_repo_url flathub/flathub)"
    info "Cloning upstream ${UPSTREAM_URL} branch new-pr (required source of truth)…"
    git clone --depth 1 -b new-pr "${UPSTREAM_URL}" "${TMP_GH}/flathub" \
      || die "Cannot clone ${UPSTREAM_URL} @ new-pr. Check network / SSH / GitHub access."

    # Ensure a personal fork exists for the push/PR head
    info "Ensuring fork ${GH_USER}/flathub exists…"
    gh repo fork flathub/flathub --clone=false 2>/dev/null \
      || info "Fork may already exist (continuing)…"

    FORK_PUSH="$(github_fork_push_url "${GH_USER}/flathub")"
    info "Will push fork ${GH_USER}/flathub via ${GIT_AUTH}"

    # Files must be at repository ROOT (not APP_ID/ subfolder) for flathub/flathub
    (
      cd "${TMP_GH}/flathub"
      git checkout -b "${BRANCH}"
      # Drop any nested layout leftovers if re-running on a dirty tree
      rm -rf "${APP_ID}"
      cp -a "${PKG_DIR}/." .
      # Helper docs must not land in the Flathub app tree
      rm -f README-SUBMIT.md PR-BODY.md PR-TITLE.txt
      git config user.email "${GH_USER}@users.noreply.github.com"
      git config user.name "${GH_USER}"
      git add "${APP_ID}.yml" "${APP_ID}.desktop" "${APP_ID}.metainfo.xml" \
        "${APP_ID}.png" generated-sources.json flathub.json 2>/dev/null || git add -A
      git status
      git commit -m "Add ${APP_ID} (${GIT_TAG})"
      git remote remove fork 2>/dev/null || true
      git remote add fork "${FORK_PUSH}"
      git push -u fork "HEAD:refs/heads/${BRANCH}"
      # Official checklist body (base new-pr). Video URL via FLATHUB_VIDEO_URL.
      write_flathub_pr_body "${TMP_GH}/PR-BODY.md"
      printf '%s\n' "${PR_TITLE}" > "${TMP_GH}/PR-TITLE.txt"
      info "PR title/description (copy if create fails)…"
      print_pr_copy_block "${PR_TITLE}" "${TMP_GH}/PR-BODY.md"
      if gh pr create \
        --repo flathub/flathub \
        --base new-pr \
        --head "${GH_USER}:${BRANCH}" \
        --title "${PR_TITLE}" \
        --body-file "${TMP_GH}/PR-BODY.md"
      then
        ok "PR opened (check GitHub) — base branch must be new-pr"
      else
        echo
        echo "ERROR: gh pr create failed. Use the title/description printed above"
        echo "to open the PR manually on GitHub (base = new-pr)."
        echo "  title file: ${PKG_DIR}/PR-TITLE.txt"
        echo "  body file:  ${PKG_DIR}/PR-BODY.md"
        exit 1
      fi
    )
  else
    info "Preparing update PR against flathub/${APP_ID}…"
    if ! gh repo view "flathub/${APP_ID}" >/dev/null 2>&1; then
      die "Repo flathub/${APP_ID} not found — app not on Flathub yet; use MODE=first"
    fi
    # Update PR body is shorter than first-submit checklist.
    PR_TITLE="Update ${APP_ID} to ${GIT_TAG}"
    printf '%s\n' "${PR_TITLE}" > "${PKG_DIR}/PR-TITLE.txt"
    cat > "${PKG_DIR}/PR-BODY.md" <<EOF
Update **${APP_ID}** to upstream \`${GIT_TAG}\` (\`${GIT_COMMIT}\`).

Upstream: ${UPSTREAM_WEB}
EOF
    APP_UPSTREAM="$(github_repo_url "flathub/${APP_ID}")"
    git clone --depth 1 "${APP_UPSTREAM}" "${TMP_GH}/app" \
      || die "Cannot clone flathub/${APP_ID} via ${GIT_AUTH}"
    (
      cd "${TMP_GH}/app"
      git checkout -b "${BRANCH}"
    )
    rsync -a --delete \
      --exclude .git \
      --exclude README-SUBMIT.md \
      --exclude PR-BODY.md \
      --exclude PR-TITLE.txt \
      "${PKG_DIR}/" "${TMP_GH}/app/"
    FORK_PUSH="$(github_fork_push_url "${GH_USER}/${APP_ID}")"
    (
      cd "${TMP_GH}/app"
      git config user.email "${GH_USER}@users.noreply.github.com"
      git config user.name "${GH_USER}"
      git add -A
      git commit -m "Update ${APP_ID} to ${GIT_TAG}" || die "Nothing to commit?"
      # Push to personal fork, open PR into flathub app repo
      gh repo fork "flathub/${APP_ID}" --clone=false 2>/dev/null || true
      git remote remove fork 2>/dev/null || true
      git remote add fork "${FORK_PUSH}"
      git push -u fork "HEAD:${BRANCH}"
      info "PR title/description (copy if create fails)…"
      print_pr_copy_block "${PR_TITLE}" "${PKG_DIR}/PR-BODY.md"
      if gh pr create \
        --repo "flathub/${APP_ID}" \
        --head "${GH_USER}:${BRANCH}" \
        --title "${PR_TITLE}" \
        --body-file "${PKG_DIR}/PR-BODY.md"
      then
        ok "Update PR opened"
      else
        echo
        echo "ERROR: gh pr create failed. Use the title/description printed above"
        echo "to open the PR manually."
        echo "  title file: ${PKG_DIR}/PR-TITLE.txt"
        echo "  body file:  ${PKG_DIR}/PR-BODY.md"
        exit 1
      fi
    )
    ok "Update PR flow finished"
  fi
fi

# ── summary ──────────────────────────────────────────────────────────────────

cat <<EOF

╔══════════════════════════════════════════════════════════════╗
║  Done                                                        ║
╚══════════════════════════════════════════════════════════════╝

Package directory:
  ${PKG_DIR}

Contents:
  ${APP_ID}.yml          (git tag ${GIT_TAG} @ ${GIT_COMMIT})
  generated-sources.json
  desktop + metainfo + icons
  PR-TITLE.txt / PR-BODY.md   (copy-paste for manual PR)

Next steps if you did NOT open a PR (or gh failed):
  1) Tag ${GIT_TAG} was ensured on origin (auto-created/pushed if it was missing)
  2) First app (order matters):
       a. Clone upstream new-pr:
            git clone -b new-pr git@github.com:flathub/flathub.git
            # or HTTPS: https://github.com/flathub/flathub.git
       b. Branch from new-pr, copy packaging files to repo ROOT
          (skip README-SUBMIT.md PR-BODY.md PR-TITLE.txt)
       c. Commit, push to YOUR fork, open PR base = new-pr
          Title/body: see block below (or ${PKG_DIR}/PR-*.txt/md)
  3) Or re-run with OPEN_PR=1:
       GIT_AUTH=ssh OPEN_PR=1 ./scripts/publish-flathub-podman.sh

Docs: flatpak/README.vi.md
Local test (host): ./scripts/publish-flatpak.sh

EOF

# Final reprint so title/description is the last thing on screen.
print_pr_copy_block "${PR_TITLE}" "${PKG_DIR}/PR-BODY.md"
