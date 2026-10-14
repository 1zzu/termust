#!/usr/bin/env bash
#
# Runs the live SSH tests (src-tauri/src/ssh/live.rs) against disposable OpenSSH
# servers, so the manual verification they replaced can be repeated by anyone
# with Docker. It never talks to a server the user did not start.
#
# Two servers are built from tests/integration/ssh and published on a dynamic
# port of 127.0.0.1:
#
#   1. an ordinary one, for password, Ed25519 and RSA SHA-2 authentication plus
#      the approved, rejected and changed host-key cases;
#   2. one that only accepts the legacy ssh-rsa (SHA-1) signature, which the
#      client has to refuse.
#
# The password and both keys are generated into a temporary directory, mounted
# read-only, and removed by the trap below together with the containers — when
# the tests pass, when they fail, and when the script is interrupted. No secret
# is ever printed.
#
# Exit status: 0 when every test passed, 1 when a test failed, 2 when the tests
# could not run at all (Docker or the OpenSSH client tools are missing), 130 when
# the script was interrupted.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/.." && pwd)"

readonly ssh_dir="$root_dir/tests/integration/ssh"
readonly image="termust-ssh-test:local"
readonly server_container="termust-ssh-test-server"
readonly legacy_container="termust-ssh-test-legacy"
readonly test_user="tester"
readonly test_log_name="cargo-test-integration.log"

work_dir=""

info() { printf '%s\n' "$*"; }

die() {
	printf 'error: %s\n' "$1" >&2
	exit "${2:-2}"
}

# Removes the containers and the throwaway credentials however the script ends.
cleanup() {
	local status=$?
	docker rm --force --volumes "$server_container" "$legacy_container" >/dev/null 2>&1 || true
	if [ -n "$work_dir" ]; then
		rm -rf "$work_dir"
	fi
	trap - EXIT
	exit "$status"
}
trap cleanup EXIT

# Ctrl+C and a cancelled job must not look like a passing run. Both exit through
# the trap above, which still removes everything.
trap 'exit 130' INT
trap 'exit 143' TERM

# --- prerequisites ---------------------------------------------------------

command -v docker >/dev/null 2>&1 || die 'Docker is required to run the live SSH tests'
docker info >/dev/null 2>&1 || die 'Docker is installed but not running'
command -v ssh-keygen >/dev/null 2>&1 || die 'the OpenSSH client tools are required (ssh-keygen)'
command -v openssl >/dev/null 2>&1 || die 'openssl is required to generate the throwaway password'

# --- throwaway credentials -------------------------------------------------

work_dir="$(mktemp -d)"
chmod 0700 "$work_dir"
password="$(openssl rand -hex 16)"

# Fresh keys every run, so no key material ever reaches the repository.
ssh-keygen -q -t ed25519 -N '' -C 'termust integration test' -f "$work_dir/id_ed25519"
ssh-keygen -q -t rsa -b 3072 -N '' -C 'termust integration test' -f "$work_dir/id_rsa"
cat "$work_dir/id_ed25519.pub" "$work_dir/id_rsa.pub" >"$work_dir/authorized_keys"
printf '%s:%s\n' "$test_user" "$password" >"$work_dir/password"
chmod 0600 "$work_dir/password" "$work_dir/id_ed25519" "$work_dir/id_rsa"

# --- helpers ---------------------------------------------------------------

# Starts one server and prints the port Docker picked for it. The second
# argument turns on the container that only accepts the legacy ssh-rsa
# signature.
start_server() {
	local name="$1"
	local legacy_rsa_only="$2"

	docker run --detach --rm \
		--name "$name" \
		--publish 127.0.0.1::22 \
		--mount "type=bind,source=$work_dir,target=/credentials,readonly" \
		--env "TERMUST_TEST_LEGACY_RSA_ONLY=$legacy_rsa_only" \
		"$image" >/dev/null

	docker port "$name" 22/tcp | tail -n 1 | sed 's/.*://'
}

# Waits until the server completes a real key exchange, instead of sleeping and
# hoping. Fails after 30 seconds.
wait_for_server() {
	local port="$1"
	local deadline=$((SECONDS + 30))

	while [ "$SECONDS" -lt "$deadline" ]; do
		if [ -n "$(ssh-keyscan -T 2 -t ed25519 -p "$port" 127.0.0.1 2>/dev/null)" ]; then
			return 0
		fi
		sleep 0.2
	done

	return 1
}

# The Ed25519 fingerprint OpenSSH sees, which the app has to report as well.
host_key_fingerprint() {
	local port="$1"

	ssh-keyscan -T 2 -t ed25519 -p "$port" 127.0.0.1 2>/dev/null |
		ssh-keygen -lf - | awk '{ print $2 }'
}

# Runs the live tests the arguments select: the first is the test-name filter,
# the rest go to the test binary. Their output is kept back until something
# fails, so a passing run stays quiet and a failing one is shown whole.
run_live_tests() {
	local filter="$1"
	shift
	local log="$work_dir/$test_log_name"

	info "cargo test $filter -- --ignored $*"
	if ! cargo test --manifest-path "$root_dir/src-tauri/Cargo.toml" "$filter" \
		-- --ignored --nocapture "$@" >"$log" 2>&1; then
		cat "$log" >&2
		return 1
	fi

	grep -E '^(test |host key approved:|test result:)' "$log"
}

# --- run -------------------------------------------------------------------

info 'building the test server image'
docker build --quiet --tag "$image" "$ssh_dir" >/dev/null

export TERMUST_LIVE_HOST=127.0.0.1
export TERMUST_LIVE_USER="$test_user"
export TERMUST_LIVE_PASSWORD="$password"

start_port="$(start_server "$server_container" 0)"
export TERMUST_LIVE_PORT="$start_port"
export TERMUST_LIVE_PRIVATE_KEY_PATH="$work_dir/id_ed25519"

if ! wait_for_server "$start_port"; then
	die "the test server never answered on 127.0.0.1:$start_port" 1
fi
fingerprint="$(host_key_fingerprint "$start_port")"
info "test server ready on 127.0.0.1:$start_port (host key $fingerprint)"

info '1/3 password, Ed25519, RSA SHA-2 and host-key policy'
# The legacy policy test needs its own server, so it runs last.
run_live_tests live_ --skip live_rsa_only || die 'the live SSH tests failed' 1

# This also proves the tests really ran: a skipped session prints no fingerprint.
if ! grep -qF "$fingerprint" "$work_dir/$test_log_name"; then
	die "the app did not report the host key fingerprint OpenSSH sees ($fingerprint)" 1
fi

info '2/3 RSA key, SHA-2 signature'
export TERMUST_LIVE_PRIVATE_KEY_PATH="$work_dir/id_rsa"
run_live_tests live_private_key || die 'the live RSA test failed' 1

info '3/3 a server that only offers the legacy ssh-rsa (SHA-1) signature'
legacy_port="$(start_server "$legacy_container" 1)"
export TERMUST_LIVE_PORT="$legacy_port"

if ! wait_for_server "$legacy_port"; then
	die "the legacy test server never answered on 127.0.0.1:$legacy_port" 1
fi
run_live_tests live_rsa_only || die 'the live legacy-policy test failed' 1

info 'all live SSH tests passed'
