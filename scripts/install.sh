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

has_library() {
    library="$1"

    if command -v ldconfig >/dev/null 2>&1 \
        && ldconfig -p 2>/dev/null | grep -Fq "${library}"; then
        return 0
    fi

    find /lib /usr/lib -name "${library}" -print -quit 2>/dev/null | grep -q .
}

check_dependencies() {
    NEED_PACTL="false"
    NEED_PULSE="false"
    NEED_ALSA="false"

    command -v pactl >/dev/null 2>&1 || NEED_PACTL="true"
    has_library libpulse.so.0 || NEED_PULSE="true"
    has_library libasound.so.2 || NEED_ALSA="true"

    [ "${NEED_PACTL}" = "false" ] \
        && [ "${NEED_PULSE}" = "false" ] \
        && [ "${NEED_ALSA}" = "false" ]
}

show_missing_dependencies() {
    echo "Missing system dependencies:"
    [ "${NEED_PACTL}" = "false" ] || echo "  - pactl"
    [ "${NEED_PULSE}" = "false" ] || echo "  - libpulse.so.0"
    [ "${NEED_ALSA}" = "false" ] || echo "  - libasound.so.2"
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
        echo "Installing dependencies requires root privileges or sudo" >&2
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
    set --
    [ "${NEED_PACTL}" = "false" ] || set -- "$@" pulseaudio-utils
    [ "${NEED_PULSE}" = "false" ] || set -- "$@" libpulse0
    if [ "${NEED_ALSA}" = "true" ]; then
        if apt-cache show libasound2t64 >/dev/null 2>&1; then
            set -- "$@" libasound2t64
        else
            set -- "$@" libasound2
        fi
    fi

    run_as_root apt-get update
    run_as_root env DEBIAN_FRONTEND=noninteractive apt-get install -y "$@"
}

install_with_dnf() {
    set --
    [ "${NEED_PACTL}" = "false" ] || set -- "$@" pulseaudio-utils
    [ "${NEED_PULSE}" = "false" ] || set -- "$@" pulseaudio-libs
    [ "${NEED_ALSA}" = "false" ] || set -- "$@" alsa-lib
    run_as_root dnf install -y "$@"
}

install_with_yum() {
    set --
    [ "${NEED_PACTL}" = "false" ] || set -- "$@" pulseaudio-utils
    [ "${NEED_PULSE}" = "false" ] || set -- "$@" pulseaudio-libs
    [ "${NEED_ALSA}" = "false" ] || set -- "$@" alsa-lib
    run_as_root yum install -y "$@"
}

install_with_pacman() {
    set --
    if [ "${NEED_PACTL}" = "true" ] || [ "${NEED_PULSE}" = "true" ]; then
        set -- "$@" libpulse
    fi
    [ "${NEED_ALSA}" = "false" ] || set -- "$@" alsa-lib
    run_as_root pacman -S --needed --noconfirm "$@"
}

install_with_zypper() {
    set --
    [ "${NEED_PACTL}" = "false" ] || set -- "$@" pulseaudio-utils
    [ "${NEED_PULSE}" = "false" ] || set -- "$@" libpulse0
    [ "${NEED_ALSA}" = "false" ] || set -- "$@" libasound2
    run_as_root zypper --non-interactive install --no-recommends "$@"
}

install_with_apk() {
    set --
    [ "${NEED_PACTL}" = "false" ] || set -- "$@" pulseaudio-utils
    [ "${NEED_PULSE}" = "false" ] || set -- "$@" libpulse
    [ "${NEED_ALSA}" = "false" ] || set -- "$@" alsa-lib
    run_as_root apk add "$@"
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
case ":${PATH}:" in
    *:"${INSTALL_DIR}":*) ;;
    *) echo "Add ${INSTALL_DIR} to PATH to run 'remotemic' from any directory." ;;
esac
