# A throwaway OpenSSH server for the live SSH tests.
#
# Nothing secret is baked into this image: the integration script generates the
# keys and the password into a temporary directory and mounts it read-only at
# /credentials when the container starts. See README.md next to this file.

# Pinned to the Debian release, so the test server does not change underneath us.
FROM debian:13-slim

RUN apt-get update \
    && apt-get install --yes --no-install-recommends openssh-server \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --shell /bin/bash tester \
    && mkdir -p /run/sshd

COPY sshd_config /etc/ssh/sshd_config
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod 0755 /usr/local/bin/entrypoint.sh

EXPOSE 22
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
