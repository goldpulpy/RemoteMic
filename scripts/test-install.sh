#!/bin/sh

set -eu

TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "${TEST_ROOT}"' EXIT HUP INT TERM

OUTPUT="$(
    # Variables in this block are intentionally expanded by the isolated child shell.
    # shellcheck disable=SC2016
    XDG_CONFIG_HOME="${TEST_ROOT}/config" TEST_INSTALL_ROOT="${TEST_ROOT}" sh -c '
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

        set -- --skip-dependencies --autostart --install-dir "${TEST_INSTALL_ROOT}/bin"
        . scripts/install.sh
    ' 2>&1
)"

test -x "${TEST_ROOT}/bin/remotemic"
test -f "${TEST_ROOT}/config/systemd/user/remotemic.service"
printf '%s\n' "${OUTPUT}" | grep -Fq "WARNING: The systemd user session is unavailable."
printf '%s\n' "${OUTPUT}" | grep -Fq "Installed RemoteMic"

echo "Installer fallback test passed"
