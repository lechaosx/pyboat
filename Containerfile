FROM scratch
COPY --chmod=755 target/aarch64-unknown-linux-musl/release/server /server
EXPOSE 8443/udp
ENTRYPOINT ["/server"]
