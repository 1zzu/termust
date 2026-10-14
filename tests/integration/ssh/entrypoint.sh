#!/bin/sh
# Prepares a disposable sshd from the read-only /credentials directory and then
# runs it in the foreground. Nothing here is baked into the image.

set -eu

install -d -m 0700 -o tester -g tester /home/tester/.ssh
install -m 0600 -o tester -g tester /credentials/authorized_keys /home/tester/.ssh/authorized_keys
chpasswd < /credentials/password

# The container is thrown away after one test run, so the host keys only need to
# exist for that run.
ssh-keygen -A

# The integration script starts a second server with the legacy policy, to prove
# that the client refuses a server offering only the ssh-rsa (SHA-1) signature.
if [ "${TERMUST_TEST_LEGACY_RSA_ONLY:-0}" = "1" ]; then
	exec /usr/sbin/sshd -D -e -f /etc/ssh/sshd_config -o PubkeyAcceptedAlgorithms=ssh-rsa
fi

exec /usr/sbin/sshd -D -e -f /etc/ssh/sshd_config
