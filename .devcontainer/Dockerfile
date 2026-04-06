FROM --platform=$BUILDPLATFORM mcr.microsoft.com/devcontainers/base:debian

RUN apt-get update  \
    && apt-get -y --no-install-recommends install  \
        sudo curl git ca-certificates build-essential fish \
    && rm -rf /var/lib/apt/lists/*

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

USER vscode
RUN curl https://mise.run | sh
RUN $HOME/.local/bin/mise use -g lazygit
RUN mkdir -p ~/.config/fish && echo '$HOME/.local/bin/mise activate fish --shims | source' >> ~/.config/fish/config.fish
# cargo node pnpm cargo:cargo-component
