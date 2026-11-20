# 发版用的镜像：直接装发布页上那份 Linux 二进制（.github/workflows/release.yml 准备好构建目录），
# 和仓库根目录的 Dockerfile（从源码编）得到的镜像一样。
FROM gcr.io/distroless/cc-debian12:nonroot
ARG TARGETARCH
COPY ${TARGETARCH}/open-agent-docs /usr/local/bin/open-agent-docs
COPY --chown=65532:65532 data /data
ENV OAD_HOST=0.0.0.0 \
    OAD_PORT=8848 \
    OAD_DATA_DIR=/data
VOLUME /data
EXPOSE 8848
USER nonroot
ENTRYPOINT ["/usr/local/bin/open-agent-docs"]
CMD ["serve"]
