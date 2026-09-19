#!/bin/sh

set -eu

REPOSITORY="goldpulpy/RemoteMic"
ASSET="remotemic-linux-x86_64"
VERSION="latest"
VERSION_SET="false"
INSTALL_DIR="${INSTALL_DIR:-${HOME}/.local/bin}"
ASSUME_YES="false"
SKIP_DEPENDENCIES="false"
DEPENDENCIES_ONLY="false"
AUTOSTART="false"
MODIFY_PATH="true"

usage() {
    cat <<'EOF'
Install RemoteMic for Linux x86_64.

Usage:
  install.sh [VERSION] [OPTIONS]
  install.sh --version VERSION [OPTIONS]

Arguments:
  VERSION              Release version such as 1.2.3 or v1.2.3 (default: latest)
  --install-dir DIR    Destination directory (default: $HOME/.local/bin)
  -y, --yes            Install missing system dependencies without asking
  --skip-dependencies  Do not check or install system dependencies
  --dependencies-only  Check/install dependencies without downloading RemoteMic
  --autostart          Enable startup with systemd, OpenRC, or runit
  --no-modify-path     Do not add $HOME/.local/bin to the shell configuration
  -h, --help           Show this help

The INSTALL_DIR environment variable can also set the destination directory.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --version)
            [ "$#" -ge 2 ] || {
                echo "--version requires a value" >&2
                exit 2
            }
            if [ "${VERSION_SET}" = "true" ]; then
                echo "Only one version may be specified" >&2
                exit 2
            fi
            VERSION="$2"
            VERSION_SET="true"
            shift 2
            ;;
        --install-dir)
            [ "$#" -ge 2 ] || {
                echo "--install-dir requires a value" >&2
                exit 2
            }
            INSTALL_DIR="$2"
            shift 2
            ;;
        -y|--yes)
            ASSUME_YES="true"
            shift
            ;;
        --skip-dependencies)
            SKIP_DEPENDENCIES="true"
            shift
            ;;
        --dependencies-only)
            DEPENDENCIES_ONLY="true"
            shift
            ;;
        --autostart)
            AUTOSTART="true"
            shift
            ;;
        --no-modify-path)
            MODIFY_PATH="false"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        -*)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
        *)
            if [ "${VERSION_SET}" = "true" ]; then
                echo "Only one version may be specified" >&2
                exit 2
            fi
            VERSION="$1"
            VERSION_SET="true"
            shift
            ;;
    esac
done

if [ "${VERSION}" = "latest" ]; then
    RELEASE_PATH="latest/download"
    DISPLAY_VERSION="latest"
else
    if ! printf '%s\n' "${VERSION}" | grep -Eq '^v?[0-9]+\.[0-9]+\.[0-9]+$'; then
        echo "Version must be 'latest' or use the MAJOR.MINOR.PATCH format" >&2
        exit 2
    fi
    case "${VERSION}" in
        v*) TAG="${VERSION}" ;;
        *) TAG="v${VERSION}" ;;
    esac
    RELEASE_PATH="download/${TAG}"
    DISPLAY_VERSION="${TAG}"
fi

case "$(uname -s):$(uname -m)" in
    Linux:x86_64|Linux:amd64) ;;
    *)
        echo "This installer currently supports Linux x86_64 only" >&2
        exit 1
        ;;
esac

check_dependencies() {
    command -v pactl >/dev/null 2>&1
}

show_missing_dependencies() {
    echo "Missing system dependencies:"
    echo "  - pactl"
}

detect_package_manager() {
    for manager in apt-get dnf yum pacman zypper apk; do
        if command -v "${manager}" >/dev/null 2>&1; then
            printf '%s\n' "${manager}"
            return 0
        fi
    done
    return 1
}

run_as_root() {
    if [ "$(id -u)" -eq 0 ]; then
        "$@"
    elif command -v sudo >/dev/null 2>&1; then
        sudo "$@"
    else
        echo "This operation requires root privileges or sudo" >&2
        return 1
    fi
}

confirm_dependency_install() {
    [ "${ASSUME_YES}" = "false" ] || return 0

    if [ ! -r /dev/tty ]; then
        echo "Cannot ask for confirmation. Run again with --yes to install dependencies." >&2
        return 1
    fi

    printf 'Install the missing dependencies now? [Y/n] ' >/dev/tty
    answer=""
    IFS= read -r answer </dev/tty || return 1
    case "${answer}" in
        ""|y|Y|yes|YES|Yes) return 0 ;;
        *) return 1 ;;
    esac
}

install_with_apt() {
    run_as_root apt-get update
    run_as_root env DEBIAN_FRONTEND=noninteractive apt-get install -y pulseaudio-utils
}

install_with_dnf() {
    run_as_root dnf install -y pulseaudio-utils
}

install_with_yum() {
    run_as_root yum install -y pulseaudio-utils
}

install_with_pacman() {
    run_as_root pacman -S --needed --noconfirm libpulse
}

install_with_zypper() {
    run_as_root zypper --non-interactive install --no-recommends pulseaudio-utils
}

install_with_apk() {
    run_as_root apk add pulseaudio-utils
}

ensure_dependencies() {
    if check_dependencies; then
        echo "System dependencies are already installed."
        return 0
    fi

    show_missing_dependencies
    package_manager="$(detect_package_manager || true)"
    if [ -z "${package_manager}" ]; then
        echo "No supported package manager found." >&2
        echo "Install the dependencies manually or use --skip-dependencies." >&2
        return 1
    fi

    echo "Detected package manager: ${package_manager}"
    if ! confirm_dependency_install; then
        echo "Dependency installation cancelled." >&2
        echo "Use --skip-dependencies to install only the RemoteMic binary." >&2
        return 1
    fi

    case "${package_manager}" in
        apt-get) install_with_apt ;;
        dnf) install_with_dnf ;;
        yum) install_with_yum ;;
        pacman) install_with_pacman ;;
        zypper) install_with_zypper ;;
        apk) install_with_apk ;;
    esac

    if ! check_dependencies; then
        show_missing_dependencies >&2
        echo "Dependencies are still missing after package installation." >&2
        return 1
    fi
    echo "System dependencies installed successfully."
}

if [ "${SKIP_DEPENDENCIES}" = "true" ] && [ "${DEPENDENCIES_ONLY}" = "true" ]; then
    echo "--skip-dependencies and --dependencies-only cannot be used together" >&2
    exit 2
fi

if [ "${DEPENDENCIES_ONLY}" = "true" ] && [ "${AUTOSTART}" = "true" ]; then
    echo "--dependencies-only and --autostart cannot be used together" >&2
    exit 2
fi

if [ "${AUTOSTART}" = "true" ]; then
    case "${INSTALL_DIR}" in
        /*) ;;
        *)
            echo "--install-dir must be an absolute path when using --autostart" >&2
            exit 2
            ;;
    esac
fi

if [ "${AUTOSTART}" = "true" ] && [ "$(id -u)" -eq 0 ]; then
    echo "Run the installer without sudo when using --autostart." >&2
    echo "It will request sudo only when enabling boot-time startup." >&2
    exit 1
fi

if [ "${SKIP_DEPENDENCIES}" = "false" ]; then
    ensure_dependencies
fi

[ "${DEPENDENCIES_ONLY}" = "false" ] || exit 0

BASE_URL="https://github.com/${REPOSITORY}/releases/${RELEASE_PATH}"
TEMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TEMP_DIR}"' EXIT HUP INT TERM

download() {
    url="$1"
    output="$2"

    if command -v curl >/dev/null 2>&1; then
        curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error \
            "${url}" --output "${output}"
    elif command -v wget >/dev/null 2>&1; then
        wget --https-only --quiet "${url}" --output-document="${output}"
    else
        echo "Install curl or wget and try again" >&2
        exit 1
    fi
}

echo "Downloading RemoteMic ${DISPLAY_VERSION}..."
download "${BASE_URL}/${ASSET}" "${TEMP_DIR}/${ASSET}"
download "${BASE_URL}/${ASSET}.sha256" "${TEMP_DIR}/${ASSET}.sha256"

if ! command -v sha256sum >/dev/null 2>&1; then
    echo "sha256sum is required to verify the download" >&2
    exit 1
fi

(cd "${TEMP_DIR}" && sha256sum --check "${ASSET}.sha256")
mkdir -p "${INSTALL_DIR}"
install -m 0755 "${TEMP_DIR}/${ASSET}" "${INSTALL_DIR}/remotemic"

echo "Installed RemoteMic to ${INSTALL_DIR}/remotemic"

enable_systemd_autostart() {
    if ! command -v systemctl >/dev/null 2>&1; then
        return 1
    fi

    config_home="${XDG_CONFIG_HOME:-${HOME}/.config}"
    service_dir="${config_home}/systemd/user"
    service_file="${service_dir}/remotemic.service"
    generated_service="${TEMP_DIR}/remotemic.service"
    binary_path="${INSTALL_DIR}/remotemic"
    escaped_binary_path="$(
        printf '%s' "${binary_path}" | sed 's/\\/\\\\/g; s/"/\\"/g; s/%/%%/g'
    )"

    if ! mkdir -p "${service_dir}"; then
        return 1
    fi
    if ! cat >"${generated_service}" <<EOF
[Unit]
Description=RemoteMic virtual microphone
Documentation=https://github.com/${REPOSITORY}
After=pipewire-pulse.service pulseaudio.service

[Service]
Type=simple
ExecStart="${escaped_binary_path}"
Restart=on-failure
RestartSec=5s
TimeoutStopSec=10s

[Install]
WantedBy=default.target
EOF
    then
        echo "WARNING: Could not generate the systemd unit; autostart was skipped." >&2
        return 1
    fi

    if [ -f "${service_file}" ] && ! cmp -s "${generated_service}" "${service_file}"; then
        backup_file="${service_file}.backup.$(date +%Y%m%d%H%M%S)"
        if ! cp -p "${service_file}" "${backup_file}"; then
            echo "WARNING: Could not back up the existing service; it was not replaced." >&2
            return 1
        fi
        echo "Backed up the existing service to ${backup_file}"
    fi
    if ! install -m 0644 "${generated_service}" "${service_file}"; then
        echo "WARNING: Could not install the systemd unit; autostart was skipped." >&2
        return 1
    fi

    if ! systemctl --user daemon-reload; then
        echo "WARNING: The systemd user session is unavailable." >&2
        return 1
    fi
    if ! systemctl --user enable remotemic.service; then
        echo "WARNING: Could not enable remotemic.service; the binary is still installed." >&2
        return 1
    fi
    if ! systemctl --user restart remotemic.service; then
        echo "WARNING: remotemic.service is enabled but could not be started now." >&2
        echo "Inspect it with: systemctl --user status remotemic.service" >&2
    fi

    if ! command -v loginctl >/dev/null 2>&1; then
        echo "WARNING: loginctl was not found; the service will start only after login." >&2
    elif ! run_as_root loginctl enable-linger "$(id -un)"; then
        echo "WARNING: Could not enable linger; the service will start only after login." >&2
    fi

    echo "Enabled RemoteMic autostart at ${service_file}"
    echo "View logs with: journalctl --user -u remotemic.service -b"
    return 0
}

enable_openrc_autostart() {
    if ! command -v rc-service >/dev/null 2>&1 \
        || ! command -v rc-update >/dev/null 2>&1 \
        || [ ! -x /sbin/openrc-run ]; then
        return 1
    fi

    service_file="/etc/init.d/remotemic"
    generated_service="${TEMP_DIR}/remotemic.openrc"
    binary_path="${INSTALL_DIR}/remotemic"
    service_user="$(id -un)"
    service_group="$(id -gn)"
    service_uid="$(id -u)"
    # shellcheck disable=SC2016
    escaped_binary_path="$(printf '%s' "${binary_path}" | sed 's/\\/\\\\/g; s/"/\\"/g; s/`/\\`/g; s/\$/\\$/g')"
    # shellcheck disable=SC2016
    escaped_home="$(printf '%s' "${HOME}" | sed 's/\\/\\\\/g; s/"/\\"/g; s/`/\\`/g; s/\$/\\$/g')"

    if ! cat >"${generated_service}" <<EOF
#!/sbin/openrc-run

name="RemoteMic"
description="RemoteMic virtual microphone"
command="${escaped_binary_path}"
command_user="${service_user}:${service_group}"
directory="${escaped_home}"
supervisor=supervise-daemon
respawn_delay=5
respawn_max=0
output_log="/var/log/remotemic.log"
error_log="/var/log/remotemic.log"
export HOME="${escaped_home}"
if [ -d "/run/user/${service_uid}" ]; then
    export XDG_RUNTIME_DIR="/run/user/${service_uid}"
else
    unset XDG_RUNTIME_DIR
fi

depend() {
    need net
    after pulseaudio pipewire pipewire-pulse
}
EOF
    then
        return 1
    fi

    if [ -f "${service_file}" ]; then
        backup_file="${service_file}.backup.$(date +%Y%m%d%H%M%S)"
        if ! run_as_root cp -p "${service_file}" "${backup_file}"; then
            echo "WARNING: Could not back up the existing OpenRC service." >&2
            return 1
        fi
        echo "Backed up the existing service to ${backup_file}"
    fi
    if ! run_as_root install -m 0755 "${generated_service}" "${service_file}"; then
        return 1
    fi
    if ! run_as_root rc-update add remotemic default; then
        return 1
    fi
    if ! run_as_root rc-service remotemic restart; then
        if ! run_as_root rc-service remotemic start; then
            echo "WARNING: The OpenRC service is enabled but could not be started now." >&2
        fi
    fi

    echo "Enabled RemoteMic autostart with OpenRC at ${service_file}"
    echo "View logs with: tail -f /var/log/remotemic.log"
    return 0
}

enable_runit_autostart() {
    if ! command -v sv >/dev/null 2>&1 || ! command -v chpst >/dev/null 2>&1; then
        return 1
    fi

    if [ -d /var/service ]; then
        active_services="/var/service"
    elif [ -d /service ]; then
        active_services="/service"
    elif [ -d /etc/runit/runsvdir/default ]; then
        active_services="/etc/runit/runsvdir/default"
    else
        return 1
    fi

    service_dir="/etc/sv/remotemic"
    generated_run="${TEMP_DIR}/remotemic-runit-run"
    generated_log_run="${TEMP_DIR}/remotemic-runit-log-run"
    binary_path="${INSTALL_DIR}/remotemic"
    service_user="$(id -un)"
    service_group="$(id -gn)"
    service_uid="$(id -u)"
    # shellcheck disable=SC2016
    escaped_binary_path="$(printf '%s' "${binary_path}" | sed 's/\\/\\\\/g; s/"/\\"/g; s/`/\\`/g; s/\$/\\$/g')"
    # shellcheck disable=SC2016
    escaped_home="$(printf '%s' "${HOME}" | sed 's/\\/\\\\/g; s/"/\\"/g; s/`/\\`/g; s/\$/\\$/g')"

    if ! cat >"${generated_run}" <<EOF
#!/bin/sh
exec 2>&1
export HOME="${escaped_home}"
if [ -d "/run/user/${service_uid}" ]; then
    export XDG_RUNTIME_DIR="/run/user/${service_uid}"
else
    unset XDG_RUNTIME_DIR
fi
cd "${escaped_home}"
exec chpst -u "${service_user}:${service_group}" "${escaped_binary_path}"
EOF
    then
        return 1
    fi

    if ! run_as_root mkdir -p "${service_dir}/log" /var/log/remotemic; then
        return 1
    fi
    if [ -f "${service_dir}/run" ]; then
        backup_file="${service_dir}/run.backup.$(date +%Y%m%d%H%M%S)"
        if ! run_as_root cp -p "${service_dir}/run" "${backup_file}"; then
            echo "WARNING: Could not back up the existing runit service." >&2
            return 1
        fi
        echo "Backed up the existing service to ${backup_file}"
    fi
    if ! run_as_root install -m 0755 "${generated_run}" "${service_dir}/run"; then
        return 1
    fi

    if command -v svlogd >/dev/null 2>&1; then
        if ! cat >"${generated_log_run}" <<'EOF'
#!/bin/sh
exec svlogd -tt /var/log/remotemic
EOF
        then
            return 1
        fi
        if ! run_as_root install -m 0755 "${generated_log_run}" "${service_dir}/log/run"; then
            return 1
        fi
    fi

    if ! run_as_root ln -sfn "${service_dir}" "${active_services}/remotemic"; then
        return 1
    fi
    if ! run_as_root sv up "${active_services}/remotemic"; then
        echo "WARNING: The runit service is enabled but could not be started now." >&2
    fi

    echo "Enabled RemoteMic autostart with runit at ${service_dir}"
    if command -v svlogd >/dev/null 2>&1; then
        echo "View logs with: tail -f /var/log/remotemic/current"
    fi
    return 0
}

enable_autostart() {
    if enable_systemd_autostart; then
        return 0
    fi
    if enable_openrc_autostart; then
        return 0
    fi
    if enable_runit_autostart; then
        return 0
    fi

    echo "WARNING: No supported service manager could be configured." >&2
    echo "RemoteMic was installed successfully without autostart." >&2
    echo "Supported managers: systemd, OpenRC, and runit." >&2
    return 1
}

configure_path() {
    case ":${PATH}:" in
        *:"${INSTALL_DIR}":*) return 0 ;;
    esac

    if [ "${MODIFY_PATH}" = "false" ] || [ "${INSTALL_DIR}" != "${HOME}/.local/bin" ]; then
        echo "Add ${INSTALL_DIR} to PATH to run 'remotemic' from any directory."
        return 0
    fi

    shell_name="${SHELL:-}"
    shell_name="${shell_name##*/}"
    case "${shell_name}" in
        bash)
            config_file="${HOME}/.bashrc"
            path_line="export PATH=\"\$HOME/.local/bin:\$PATH\""
            ;;
        zsh)
            config_file="${HOME}/.zshrc"
            path_line="export PATH=\"\$HOME/.local/bin:\$PATH\""
            ;;
        fish)
            config_file="${XDG_CONFIG_HOME:-${HOME}/.config}/fish/config.fish"
            path_line="fish_add_path \"\$HOME/.local/bin\""
            ;;
        sh|dash|ash|ksh)
            config_file="${HOME}/.profile"
            path_line="export PATH=\"\$HOME/.local/bin:\$PATH\""
            ;;
        *)
            echo "Could not detect a supported login shell."
            echo "Add ${INSTALL_DIR} to PATH to run 'remotemic' from any directory."
            return 0
            ;;
    esac

    config_dir="${config_file%/*}"
    if ! mkdir -p "${config_dir}"; then
        echo "WARNING: Could not create ${config_dir}; PATH was not updated." >&2
        return 0
    fi

    if [ -f "${config_file}" ] && grep -Fqx "${path_line}" "${config_file}"; then
        echo "${INSTALL_DIR} is already configured in ${config_file}."
        echo "Restart the terminal, then run 'remotemic'."
        return 0
    fi

    if ! printf '\n%s\n' "${path_line}" >>"${config_file}"; then
        echo "WARNING: Could not update ${config_file}; PATH was not updated." >&2
        echo "Add ${INSTALL_DIR} to PATH to run 'remotemic' from any directory."
        return 0
    fi

    echo "Added ${INSTALL_DIR} to PATH in ${config_file}."
    echo "Restart the terminal, then run 'remotemic'."
}

configure_path

if [ "${AUTOSTART}" = "true" ]; then
    enable_autostart
fi
