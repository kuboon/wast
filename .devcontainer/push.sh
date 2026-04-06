#!/bin/bash

# get classic token
# https://github.com/settings/tokens/new?scopes=write:packages,delete:packages&description=ghcr
USER=kuboon
NAME=wast-dev-container
REPO=ghcr.io/${USER}/${NAME}
# yes | docker login ghcr.io -u ${USER} || docker login ghcr.io -u ${USER}
docker buildx build --platform linux/amd64,linux/arm64 --push -t ${REPO}:latest .
docker push ${REPO}:latest
