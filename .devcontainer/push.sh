#!/bin/bash

# get classic token
# https://github.com/settings/tokens/new?scopes=write:packages,delete:packages&description=ghcr
USER=kuboon
REPO=wast
IMAGE=wast-dev-container
FULL=ghcr.io/${USER}/${REPO}/${IMAGE}
TAG=latest
# yes | docker login ghcr.io -u ${USER} || docker login ghcr.io -u ${USER}
docker buildx build --platform linux/amd64,linux/arm64 --push -t ${FULL}:latest .
