FROM --platform=$BUILDPLATFORM mcr.microsoft.com/devcontainers/base:debian

LABEL org.opencontainers.image.source=https://github.com/kuboon/wast
LABEL org.opencontainers.image.description="WAST development container"
LABEL org.opencontainers.image.licenses=MIT

RUN apt-get update  \
    && apt-get -y --no-install-recommends install  \
        sudo curl git ca-certificates build-essential fish \
    && rm -rf /var/lib/apt/lists/*

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

USER vscode
RUN curl https://mise.run | sh
RUN mkdir -p ~/.config/fish && echo '$HOME/.local/bin/mise activate fish --shims | source' >> ~/.config/fish/config.fish
RUN $HOME/.local/bin/mise use -g node pnpm rust cargo:cargo-component
RUN $HOME/.local/bin/mise exec -- rustup target add wasm32-wasip1
