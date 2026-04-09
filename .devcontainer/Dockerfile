FROM mcr.microsoft.com/vscode/devcontainers/base:debian

LABEL org.opencontainers.image.source=https://github.com/kuboon/wast
LABEL org.opencontainers.image.description="WAST development container"
LABEL org.opencontainers.image.licenses=MIT

RUN apt-get update  \
    && apt-get -y --no-install-recommends install  \
        sudo curl git ca-certificates build-essential openssh-client fish \
    && rm -rf /var/lib/apt/lists/*

ENV HOME=/root
ENV MISE_INSTALL_PATH=/usr/local/bin/mise
RUN curl https://mise.run | sh
RUN mkdir -p $HOME/.config/fish && echo '${MISE_INSTALL_PATH} activate fish --shims | source' >> $HOME/.config/fish/config.fish && \
    echo 'eval "$(${MISE_INSTALL_PATH} activate bash --shims)"' >> $HOME/.bashrc
COPY --parents mise.toml ./**/cargo.* ./**/package.json ./**/pnpm-lock.json ./
RUN mise trust
RUN mise install --system --before 3d
RUN . "$HOME/.cargo/env"
RUN mise x -- rustup update
RUN mise x -- cargo binstall cargo-component
COPY .devcontainer/postCreate.sh /postCreate.sh
RUN chmod +x /postCreate.sh
