# BUILD CONTAINER

FROM rust:1.93 AS build

ENV CARGO_NET_GIT_FETCH_WITH_CLI=true

RUN USER=root cargo new --bin cupcake

# Build dependencies separately for layer caching.
WORKDIR /cupcake
COPY ./Cargo.toml ./Cargo.lock ./
RUN cargo build --release

# Clean the temporary project.
RUN rm src/*.rs ./target/release/deps/cupcake*

# Build the application.
ADD . ./
RUN cargo build --release --verbose

# RUNTIME CONTAINER

FROM gcr.io/distroless/cc-debian13

COPY --from=build /cupcake/target/release/cupcake /bin/cupcake

WORKDIR /cupcake

ENV CUPCAKE_LOG_LEVEL=info

ENTRYPOINT ["/bin/cupcake"]
