FROM debian:stable-slim

LABEL org.opencontainers.image.source=https://github.com/kuboon/wast
LABEL org.opencontainers.image.description="WAST development container"
LABEL org.opencontainers.image.licenses=MIT

RUN apt-get update  \
    && apt-get -y --no-install-recommends install  \
        sudo curl git ca-certificates build-essential fish \
    && rm -rf /var/lib/apt/lists/*

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ENV HOME=/root
RUN curl https://mise.run | MISE_INSTALL_PATH=/usr/local/bin/mise sh
RUN mkdir -p $HOME/.config/fish && echo 'mise activate fish --shims | source' >> $HOME/.config/fish/config.fish && \
    echo 'eval "$(mise activate bash --shims)"' >> $HOME/.bashrc
RUN mise install --system node pnpm rust && mise use node pnpm rust
RUN mise install --system cargo:cargo-component
RUN mise exec -- rustup target add wasm32-wasip1

COPY .devcontainer/postCreate.sh /postCreate.sh
RUN chmod +x /postCreate.sh
