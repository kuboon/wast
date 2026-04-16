#!/bin/bash
set -e

sudo chmod -R +r /usr/local/share
sudo chown -R vscode:vscode $HOME
mise trust
mise run build
