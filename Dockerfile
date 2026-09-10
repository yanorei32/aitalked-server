FROM rust:1.97.1-trixie AS build-env

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN apt-get update -qq \
	&& apt-get install -qq -y --no-install-recommends gcc-mingw-w64-i686

WORKDIR /usr/src

RUN cargo new aitalked-server

COPY LICENSE \
	Cargo.toml \
	Cargo.lock \
	rust-toolchain \
	/usr/src/aitalked-server/

WORKDIR /usr/src/aitalked-server/
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
RUN	cargo install cargo-license && cargo license \
	--authors \
	--do-not-bundle \
	--avoid-dev-deps \
	--avoid-build-deps \
	--filter-platform "$(rustc -vV | sed -n 's|host: ||p')" \
	> CREDITS

RUN cargo build --release --target i686-pc-windows-gnu
COPY ./src ./assets /usr/src/aitalked-server/
RUN cargo build --release --target i686-pc-windows-gnu

FROM debian:trixie-slim

RUN dpkg --add-architecture i386 \
	&& apt-get update -qq \
	&& apt-get install -qq -y --no-install-recommends wine wine32 \
	&& rm -rf /var/lib/apt/lists/*

COPY --chown=root:root --from=build-env \
	/usr/src/aitalked-server/CREDITS \
	/usr/src/aitalked-server/LICENSE \
	/usr/share/licenses/aitalked-server/

COPY --chown=root:root --from=build-env \
	/usr/src/aitalked-server/target/i686-pc-windows-gnu/release/aitalked-server.exe \
	/usr/bin/aitalked-server.exe

CMD ["wine", "/usr/bin/aitalked-server.exe"]
