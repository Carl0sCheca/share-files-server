FROM alpine:3.18
RUN apk add git
RUN apk add build-base
RUN apk add curl
RUN apk add openssl
RUN apk add openssl-dev
RUN apk add openssl-libs-static
RUN apk add pkgconfig
ENV OPENSSL_STATIC=yes
ENV OPENSSL_LIB_DIR=/usr/lib/
ENV OPENSSL_INCLUDE_DIR=/usr/include/
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --profile minimal -y
ENV PATH="$PATH:/root/.cargo/bin"
RUN git clone https://github.com/Carl0sCheca/share-files-server
RUN mkdir -p /app
RUN cd share-files-server && cargo build --release && cp target/release/share-files-server /app/share-files-server
RUN rm -rf /share-files-server
EXPOSE 9500
ENTRYPOINT ["/app/share-files-server"]
