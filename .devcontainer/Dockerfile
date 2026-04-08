FROM mcr.microsoft.com/vscode/devcontainers/base:debian

LABEL org.opencontainers.image.source=https://github.com/kuboon/wast
LABEL org.opencontainers.image.description="WAST development container"
LABEL org.opencontainers.image.licenses=MIT

RUN apt-get update  \
    && apt-get -y --no-install-recommends install  \
        sudo curl git ca-certificates build-essential openssh-client fish \
    && rm -rf /var/lib/apt/lists/*

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ENV HOME=/root
RUN curl https://mise.run | MISE_INSTALL_PATH=/usr/local/bin/mise sh
RUN mkdir -p $HOME/.config/fish && echo 'mise activate fish --shims | source' >> $HOME/.config/fish/config.fish && \
    echo 'eval "$(mise activate bash --shims)"' >> $HOME/.bashrc
RUN mise install --system --before 3d node pnpm rust && mise use node pnpm rust
RUN mise install --system --before 3d cargo:cargo-component && mise use cargo:cargo-component
RUN mise exec -- rustup target add wasm32-wasip1
