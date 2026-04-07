#!/bin/bash

set -euo pipefail

# get classic token
# https://github.com/settings/tokens/new?scopes=write:packages,delete:packages&description=ghcr
USER=kuboon
REPO=wast
IMAGE=wast-dev-container
FULL=ghcr.io/${USER}/${REPO}/${IMAGE}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
BRANCH="$(git -C "${ROOT_DIR}" rev-parse --abbrev-ref HEAD)"

SHA="$(git -C "${ROOT_DIR}" rev-parse --short=12 HEAD)"
ARM64_TAG="arm64-${SHA}"


# yes | docker login ghcr.io -u ${USER} || docker login ghcr.io -u ${USER}
PLATFORMS="linux/arm64"

echo "Building platforms: ${PLATFORMS}"
docker buildx build \
  --no-cache \
	--platform "${PLATFORMS}" \
	--push \
	-t "${FULL}:${ARM64_TAG}" \
	-t "${FULL}:arm64-latest" \
	"${SCRIPT_DIR}"

echo "Pushed: ${FULL}:${ARM64_TAG}"

git -C "${ROOT_DIR}" fetch origin "${BRANCH}" --quiet

if ! git -C "${ROOT_DIR}" rev-parse --verify "origin/${BRANCH}" >/dev/null 2>&1; then
	echo "origin/${BRANCH} was not found. Push your branch first."
	exit 1
fi

if ! git -C "${ROOT_DIR}" diff --quiet "origin/${BRANCH}" -- .devcontainer/Dockerfile; then
	echo ".devcontainer/Dockerfile is not pushed to origin/${BRANCH}. Push it before running this script."
	exit 1
fi

if ! git -C "${ROOT_DIR}" merge-base --is-ancestor HEAD "origin/${BRANCH}"; then
	echo "Current HEAD (${SHA}) is not pushed to origin/${BRANCH}. Push your branch first."
	exit 1
fi

echo "Dispatching workflow: Publish Devcontainer Image"
gh workflow run "Publish Devcontainer Image" \
	--ref "${BRANCH}" \
	-f source_sha="${SHA}" \
	-f publish_latest=true
echo "Dispatched with source_sha=${SHA}"
