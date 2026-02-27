FROM scratch

ARG TARGETARCH
COPY uentry /uentry

ENTRYPOINT ["/uentry"]
CMD ["--help"]
