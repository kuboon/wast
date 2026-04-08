FROM debian:stable-slim

LABEL org.opencontainers.image.source=https://github.com/kuboon/wast
LABEL org.opencontainers.image.description="WAST development container"
LABEL org.opencontainers.image.licenses=MIT

RUN apt-get update  \
    && apt-get -y --no-install-recommends install  \
        sudo curl git ca-certificates build-essential fish \
    && rm -rf /var/lib/apt/lists/*

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN curl https://mise.run | sh
RUN mkdir -p $HOME/.config/fish && echo '$HOME/.local/bin/mise activate fish --shims | source' >> $HOME/.config/fish/config.fish
RUN $HOME/.local/bin/mise use -g node pnpm rust
RUN $HOME/.local/bin/mise use -g cargo:cargo-component
RUN $HOME/.local/bin/mise exec -- rustup target add wasm32-wasip1

COPY postCreate.sh /postCreate.sh
RUN chmod +x /postCreate.sh
