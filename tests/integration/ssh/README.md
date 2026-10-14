# Disposable SSH test server

A small OpenSSH server image used by `scripts/test-ssh-integration.sh`, which
runs the live tests in `src-tauri/src/ssh/live.rs` against a real server without
touching anyone's machine.

The files here are only the server:

| File            | Purpose                                                                   |
| --------------- | ------------------------------------------------------------------------- |
| `Dockerfile`    | Debian with `openssh-server` and one unprivileged `tester` account        |
| `sshd_config`   | Fixed policy: password and public-key auth, one allowed user              |
| `entrypoint.sh` | Copies the mounted credentials into place, generates host keys, runs sshd |

No credential lives in this directory. The script generates a password, an
Ed25519 key and an RSA key into a temporary directory, mounts that directory
read-only at `/credentials`, and deletes it afterwards.

## Running the tests

From the repository root:

```sh
npm run test:ssh-integration
```

Docker must be installed and running. The script builds the image, starts two
containers on `127.0.0.1` with a dynamic host port each, runs the tests, and
removes the containers and the temporary credentials whether the tests pass or
fail. See the script itself for what each scenario proves.

## Running the server by hand

Useful when reviewing the app against the same server the tests use. Create a
directory with the credentials, then start the container with that directory
mounted at `/credentials`:

```sh
mkdir -p /tmp/termust-manual
ssh-keygen -q -t ed25519 -N '' -f /tmp/termust-manual/id_ed25519
cp /tmp/termust-manual/id_ed25519.pub /tmp/termust-manual/authorized_keys
printf 'tester:%s\n' "$(openssl rand -hex 12)" > /tmp/termust-manual/password
cat /tmp/termust-manual/password

docker build -t termust-ssh-test tests/integration/ssh
docker run --rm -p 127.0.0.1:2222:22 \
    -v /tmp/termust-manual:/credentials:ro \
    termust-ssh-test
```

The `cat` prints the password for the throwaway `tester` account so you can type
it into the app; the port is fixed here so it is easy to fill in the form. Then
remove `/tmp/termust-manual` and stop the container when you are done.
