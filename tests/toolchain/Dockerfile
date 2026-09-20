# Pinned Linux environment for the heavy (real XeLaTeX/Biber) test lane,
# provisioned by Terse itself: the image runs the same `terse toolchain
# install` a user runs, from the frozen TeX Live 2025 repository pinned in
# crates/terse-core/profiles/toolchain-texlive-2025.toml. Only `docker build`
# touches the network; the test lane runs with `--network none`.
#
# Build: docker build -t terse-toolchain -f tests/toolchain/Dockerfile .
# Run:   docker run --rm --network none -v "$PWD":/work -w /work terse-toolchain scripts/test-tex.sh

FROM rust:1.98.1-slim-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
RUN cargo build --locked --release -p terse-cli --bin terse

FROM debian:bookworm-slim
# perl, wget, xz: what install-tl and tlmgr need; poppler-utils: PDF text,
# link, and raster inspection used by the e2e suite; fontconfig: XeTeX's
# font backend; build-essential/pkg-config/libssl-dev: compiling the test
# suite inside the container.
RUN apt-get update && apt-get install -y --no-install-recommends \
    perl wget xz-utils tar ca-certificates curl \
    poppler-utils fontconfig \
    build-essential pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# The default profile installs rustfmt and clippy, which rust-toolchain.toml
# lists as required components; a minimal profile would make rustup try to
# fetch them when the mounted repository is entered with the network off.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.98.1 --profile default
ENV PATH="/root/.cargo/bin:${PATH}"

COPY --from=build /src/target/release/terse /usr/local/bin/terse

# Provision the managed toolchain exactly as a user would, then prove it
# with doctor. The prefix lands in the image's user data directory
# (/root/.local/share/terse/toolchain/texlive-2025) with its lock file.
RUN terse toolchain install && terse doctor --toolchain managed

WORKDIR /work
