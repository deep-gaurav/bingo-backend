FROM debian:bookworm-slim
COPY target/release/bingo-backend /usr/local/bin/app
ENTRYPOINT ["/usr/local/bin/app"]