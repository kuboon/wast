#!/bin/bash
set -e

sudo chown -R vscode:vscode $HOME
mise trust
mise x -- cargo component build --workspace
mise x -- pnpm install
