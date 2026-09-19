#!/bin/sh

set -eu

TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "${TEST_ROOT}"' EXIT HUP INT TERM

if grep -Eq 'libasound|NEED_ALSA' scripts/install.sh; then
    echo "Installer still requires an unrelated ALSA runtime" >&2
    exit 1
fi

set +e
RELATIVE_OUTPUT="$(
    sh scripts/install.sh --skip-dependencies --autostart --install-dir relative/bin 2>&1
)"
RELATIVE_STATUS="$?"
set -e
test "${RELATIVE_STATUS}" -ne 0
printf '%s\n' "${RELATIVE_OUTPUT}" \
    | grep -Fq -- "--install-dir must be an absolute path when using --autostart"

set +e
OUTPUT="$(
    # Variables in this block are intentionally expanded by the isolated child shell.
    # shellcheck disable=SC2016
    HOME="${TEST_ROOT}/home" \
        SHELL="/bin/zsh" \
        XDG_CONFIG_HOME="${TEST_ROOT}/config" \
        sh -c '
        curl() {
            output=""
            while [ "$#" -gt 0 ]; do
                case "$1" in
                    --output)
                        output="$2"
                        shift 2
                        ;;
                    *) shift ;;
                esac
            done

            case "${output}" in
                *.sha256)
                    asset_path="${output%.sha256}"
                    checksum_output="$(sha256sum "${asset_path}")"
                    checksum="${checksum_output%% *}"
                    printf "%s  remotemic-linux-x86_64\n" "${checksum}" >"${output}"
                    ;;
                *) printf "fake RemoteMic binary\n" >"${output}" ;;
            esac
        }

        systemctl() {
            return 1
        }

        set -- --skip-dependencies --autostart
        . scripts/install.sh
    ' 2>&1
)"
STATUS="$?"
set -e

test "${STATUS}" -ne 0
test -x "${TEST_ROOT}/home/.local/bin/remotemic"
test -f "${TEST_ROOT}/config/systemd/user/remotemic.service"
grep -Fqx "export PATH=\"\$HOME/.local/bin:\$PATH\"" "${TEST_ROOT}/home/.zshrc"
printf '%s\n' "${OUTPUT}" | grep -Fq "WARNING: The systemd user session is unavailable."
printf '%s\n' "${OUTPUT}" | grep -Fq "Installed RemoteMic"
printf '%s\n' "${OUTPUT}" | grep -Fq "Added ${TEST_ROOT}/home/.local/bin to PATH"

echo "Installer fallback test passed"
