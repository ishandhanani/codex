#!/usr/bin/env bash
# cut-release.sh X.Y.Z
#
# Cuts an enforce-us release for upstream tag rust-vX.Y.Z:
#   - creates branch enforce-us-X.Y.Z from the upstream tag
#   - applies enforce-us.patch via `git am -3`
#   - verifies the patch's expected literals are present
#   - adds .github/workflows/enforce-us-release.yml in a second commit so
#     tag pushes on the branch can trigger the release workflow
#   - tags enforce-us-vX.Y.Z and pushes branch + tag to origin
#
# Exit codes:
#   0  success (or tag already exists - nothing to do)
#   1  usage / verification / push error
#   2  `git am` conflict - the am session is left in progress for the caller
set -euo pipefail

VERSION="${1:?usage: $0 X.Y.Z}"
UPSTREAM_REMOTE="${UPSTREAM_REMOTE:-upstream}"
UPSTREAM_URL="${UPSTREAM_URL:-https://github.com/openai/codex.git}"
UPSTREAM_TAG="rust-v${VERSION}"
BRANCH="enforce-us-${VERSION}"
TAG="enforce-us-v${VERSION}"
TOOLING_REF="${TOOLING_REF:-enforce-us-tooling}"
PATCH_FILE="enforce-us.patch"
WORKFLOW_FILE=".github/workflows/enforce-us-release.yml"
PATCH_LITERAL='"enforce_residency = \"us\""'

die() { echo "cut-release: error: $*" >&2; exit 1; }

# Make sure the upstream tag exists locally.
if ! git rev-parse --verify --quiet "refs/tags/${UPSTREAM_TAG}" >/dev/null; then
    if ! git remote get-url "${UPSTREAM_REMOTE}" >/dev/null 2>&1; then
        git remote add "${UPSTREAM_REMOTE}" "${UPSTREAM_URL}"
    fi
    echo "cut-release: fetching ${UPSTREAM_TAG} from ${UPSTREAM_REMOTE}"
    git fetch "${UPSTREAM_REMOTE}" "tag ${UPSTREAM_TAG}" \
        || die "upstream tag ${UPSTREAM_TAG} not found on ${UPSTREAM_REMOTE}"
fi

if git rev-parse --verify --quiet "refs/tags/${TAG}" >/dev/null; then
    echo "cut-release: ${TAG} already exists; nothing to do"
    exit 0
fi

# Extract tooling artifacts before switching branches.
patch_tmp="$(mktemp)"; wf_tmp="$(mktemp)"
trap 'rm -f "${patch_tmp}" "${wf_tmp}"' EXIT
git show "${TOOLING_REF}:${PATCH_FILE}" >"${patch_tmp}" \
    || die "cannot read ${PATCH_FILE} from ${TOOLING_REF}"
git show "${TOOLING_REF}:${WORKFLOW_FILE}" >"${wf_tmp}" \
    || die "cannot read ${WORKFLOW_FILE} from ${TOOLING_REF}"

echo "cut-release: creating ${BRANCH} from ${UPSTREAM_TAG}"
git checkout -B "${BRANCH}" "${UPSTREAM_TAG}^{commit}"

if ! git am -3 "${patch_tmp}"; then
    echo "cut-release: CONFLICT applying ${PATCH_FILE} onto ${UPSTREAM_TAG}" >&2
    echo "cut-release: am session left in progress; resolve and 'git am --continue'," >&2
    echo "cut-release: or 'git am --abort' to bail." >&2
    exit 2
fi

# Verify the patch landed as expected.
for f in codex-rs/config/src/cloud_config_bundle.rs codex-rs/config/src/loader/mod.rs; do
    grep -Fq "${PATCH_LITERAL}" "${f}" \
        || die "expected literal ${PATCH_LITERAL} missing in ${f} after apply"
done
grep -Fq 'Ok(_contents)' codex-rs/config/src/loader/mod.rs \
    || die "expected 'Ok(_contents)' missing in codex-rs/config/src/loader/mod.rs"

# The release workflow must exist at the pushed tag for tag-push triggers.
mkdir -p .github/workflows
cp "${wf_tmp}" "${WORKFLOW_FILE}"
git add "${WORKFLOW_FILE}"
git commit -m "ci: add enforce-us release workflow"

git tag -a "${TAG}" -m "enforce-us ${VERSION}"
git push origin "${BRANCH}" "refs/tags/${TAG}"

echo "cut-release: pushed ${BRANCH} and ${TAG}"
