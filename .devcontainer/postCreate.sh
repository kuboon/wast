#!/bin/bash

set -e
cd $WORKSPACE_FOLDER
useradd -m -s /bin/bash vscode
chown -R vscode:vscode $WORKSPACE_FOLDER
