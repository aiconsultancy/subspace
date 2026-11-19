FROM ghcr.io/ergochat/ergo:v2.19.1@sha256:ef885e44f7fa19101bbbc41baef11dc280dc8107465dccaf6f0860f41b48a682

# BuildKit applies this ownership to the base image's declared /ircd volume.
RUN apk add --no-cache curl jq openssl python3 \
    && addgroup -g 10001 ergo \
    && adduser -D -H -u 10001 -G ergo -s /sbin/nologin ergo \
    && mkdir -p /run/subspace-irc \
    && chown ergo:ergo /ircd /run/subspace-irc \
    && chmod 0700 /ircd /run/subspace-irc

COPY --chmod=0755 entrypoint.sh /ircd-bin/subspace-entrypoint.sh
COPY ircd.yaml.tmpl seed-channels.py subspace.motd /ircd-bin/

USER 10001:10001
EXPOSE 6667/tcp 6697/tcp 8097/tcp
ENTRYPOINT ["/ircd-bin/subspace-entrypoint.sh"]
