# build
FROM rust:slim-bullseye as builder
WORKDIR /code
RUN apt-get update \
    && apt-get install -y \
        clang libclang-dev libopencv-dev \
        libavformat-dev libavcodec-dev
COPY . .
RUN cargo b --release --no-default-features --bin screenshot \
    && strip target/release/screenshot \
    && echo "Required dynamic libraries: " \
    && ldd target/release/screenshot

# runner
FROM debian:bullseye-slim
WORKDIR /code
RUN apt update \
    && apt-get install -y libopencv-core4.5 libopencv-imgproc4.5 libopencv-imgcodecs4.5 \
            libavformat58 libavcodec58 libswscale5 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /code/target/release/screenshot .
ENTRYPOINT [ "./screenshot" ]
CMD []
