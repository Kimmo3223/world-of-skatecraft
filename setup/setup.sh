#!/usr/bin/env bash
# World of Skatecraft setup (Linux). Run from anywhere:
#
#   setup/setup.sh                       # everything, asking for the two game folders
#   setup/setup.sh --wow DIR --skate DIR # same, without asking
#   setup/setup.sh --doctor              # only check this machine
#
# Every step is skipped when its result is already there, so after a failure fix the reported
# problem and run it again. Everything is logged to setup.log.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"

# NixOS has no system libraries for the build: re-run inside the repo's nix shell.
if [ -e /etc/NIXOS ] && [ -z "${IN_NIX_SHELL:-}" ]; then
    exec nix-shell "$ROOT/shell.nix" --run "$(printf '%q ' "$0" "$@")"
fi

STATE="$ROOT/.setup"
mkdir -p "$STATE/bin"
export PATH="$STATE/bin:$PATH"
exec > >(tee -a "$ROOT/setup.log") 2>&1

# Pinned upstreams.
ENGINE_URL=https://github.com/SK8-ENGINE/skate-3-rust-engine
ENGINE_REV=cb79689
CONVERTER_URL=https://raw.githubusercontent.com/Kimmo3223/2010-rust-rewrite-mashup/adb684e78ec07654f00e2a9da88d32ae02cc68bd/skate/converter/iw4l_skate_convert.py
VGMSTREAM_URL=https://github.com/vgmstream/vgmstream/releases/download/r2117/vgmstream-linux.zip
SERVER_URL=https://github.com/mserajnik/vmangos-deploy
SERVER_REV=f0692574b7955e78d68fea39372e0045360ce5ee
SERVER_IMAGE=ghcr.io/mserajnik/vmangos-server:5875

step() { printf '\n==> %s\n' "$*"; }
die() {
    printf '\nSETUP STOPPED: %s\n' "$1"
    [ $# -gt 1 ] && printf 'Fix: %s\n' "$2"
    printf 'Then run setup/setup.sh again; finished steps are skipped.\n'
    exit 1
}

WOW_DIR="" SKATE_DIR="" DOCTOR_ONLY=0
while [ $# -gt 0 ]; do
    case "$1" in
        --wow) WOW_DIR=$2; shift 2 ;;
        --skate) SKATE_DIR=$2; shift 2 ;;
        --doctor) DOCTOR_ONLY=1; shift ;;
        -h|--help) sed -n 2,10p "$0"; exit 0 ;;
        *) die "unknown option $1" "see setup/setup.sh --help" ;;
    esac
done

# ── Doctor ─────────────────────────────────────────────────────────────────────────────────────
distro() {
    . /etc/os-release 2>/dev/null || true
    case " ${ID:-} ${ID_LIKE:-} " in
        *" nixos "*) echo nixos ;;
        *" arch "*) echo arch ;;
        *" fedora "*|*" rhel "*) echo fedora ;;
        *" debian "*|*" ubuntu "*) echo debian ;;
        *) echo other ;;
    esac
}
packages_hint() {
    case "$(distro)" in
        debian) echo "sudo apt install build-essential pkg-config libasound2-dev libudev-dev git curl python3 python3-venv docker.io docker-compose-v2" ;;
        arch) echo "sudo pacman -S --needed base-devel pkgconf alsa-lib systemd-libs git curl python docker docker-compose" ;;
        fedora) echo "sudo dnf install gcc pkgconf-pkg-config alsa-lib-devel systemd-devel git curl python3 docker docker-compose-plugin" ;;
        nixos) echo "enable Docker in configuration.nix (virtualisation.docker.enable = true;); everything else comes from shell.nix" ;;
        *) echo "install: a C compiler, pkg-config, ALSA and libudev development files, git, curl, python3 with venv, Docker with the compose plugin" ;;
    esac
}

doctor() {
    step "Checking this machine ($(distro))"
    local bad=0
    check() { # name, test command, fix
        if eval "$2" >/dev/null 2>&1; then printf '  ok       %s\n' "$1"
        else printf '  MISSING  %s\n           fix: %s\n' "$1" "$3"; bad=1; fi
    }
    check "Rust (rustup/cargo)" "command -v cargo" "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   (then open a new terminal)"
    check "C compiler" "command -v cc" "$(packages_hint)"
    check "pkg-config + ALSA + libudev" "pkg-config --exists alsa libudev" "$(packages_hint)"
    check "git" "command -v git" "$(packages_hint)"
    check "curl" "command -v curl" "$(packages_hint)"
    check "python3 with venv" "python3 -c 'import venv, ensurepip'" "$(packages_hint)"
    check "Docker" "command -v docker" "$(packages_hint)"
    check "Docker compose plugin" "docker compose version" "$(packages_hint)"
    check "Docker running and usable by you" "docker info" \
        "start Docker (sudo systemctl enable --now docker) and add yourself to its group: sudo usermod -aG docker \$USER, then log out and back in"
    local free
    free=$(df -P -BG "$ROOT" | awk 'NR==2 {gsub("G","",$4); print $4}')
    check "20 GB free disk (${free} GB)" "[ ${free:-0} -ge 20 ]" "free up disk space"
    if [ -e /dev/input/by-id ] && ! ls /dev/input/by-id/*event-joystick >/dev/null 2>&1; then
        printf '  note     no controller found right now; plug in an Xbox-style pad before playing\n'
    fi
    return $bad
}

if [ "$DOCTOR_ONLY" = 1 ]; then
    doctor && echo "All good." || exit 1
    exit 0
fi
doctor || die "this machine is missing the tools listed above" "install them, then run setup again"

ask_dir() { # var, prompt
    local value=${!1}
    while [ -z "$value" ]; do
        read -r -p "$2: " value </dev/tty
        value=${value/#\~/$HOME}
    done
    printf -v "$1" '%s' "$(realpath -m "$value")"
}

# ── WoW client ─────────────────────────────────────────────────────────────────────────────────
if [ ! -e "$ROOT/WoW/Data" ]; then
    step "WoW 1.12.1 client"
    ask_dir WOW_DIR "Path to your WoW 1.12.1 (English, build 5875) folder, the one with Data/ in it"
    [ "$(basename "$WOW_DIR")" = Data ] && WOW_DIR=$(dirname "$WOW_DIR")
    for mpq in dbc.MPQ patch.MPQ model.MPQ terrain.MPQ; do
        ls "$WOW_DIR/Data/" 2>/dev/null | grep -qix "$mpq" ||
            die "$WOW_DIR/Data has no $mpq" "point setup at an untouched English 1.12.1 client folder"
    done
    ln -sfn "$WOW_DIR" "$ROOT/WoW"
    echo "  linked WoW -> $WOW_DIR"
fi

# ── Skate 3 data ───────────────────────────────────────────────────────────────────────────────
SKATE_DIR_FILE="$STATE/skate3-path"
[ -z "$SKATE_DIR" ] && [ -f "$SKATE_DIR_FILE" ] && SKATE_DIR=$(cat "$SKATE_DIR_FILE")
need_skate() {
    ask_dir SKATE_DIR "Path to your extracted Skate 3 (Xbox 360) folder, the one with default.xex and data/"
    [ -f "$SKATE_DIR/default.xex" ] && [ -d "$SKATE_DIR/data" ] ||
        die "$SKATE_DIR has no default.xex with a data folder beside it" \
            "extract your Skate 3 disc (for example with extract-xiso) and point setup at that folder; ISO files do not work"
    echo "$SKATE_DIR" > "$SKATE_DIR_FILE"
}

if [ ! -f "$ROOT/skate-data/assets/private/skater.glb" ]; then
    step "Converting Skate 3 data (skater, animations, physics)"
    need_skate
    ENGINE="$STATE/skate-engine"
    if [ ! -d "$ENGINE/.git" ]; then git clone --quiet "$ENGINE_URL" "$ENGINE"; fi
    git -C "$ENGINE" fetch --quiet origin || true
    git -C "$ENGINE" checkout --quiet "$ENGINE_REV"
    [ -x "$STATE/venv/bin/python" ] || python3 -m venv "$STATE/venv" ||
        die "could not create a Python venv" "$(packages_hint)"
    "$STATE/venv/bin/python" -m pip install --quiet --upgrade pip
    "$STATE/venv/bin/python" -m pip install --quiet 'numpy>=2.2,<3' 'Pillow>=11.3'
    # The converters look for this file name; ctypes loads the ELF despite the extension.
    mkdir -p "$ENGINE/target/native"
    rustc --edition 2024 --crate-type cdylib -C opt-level=3 -C panic=abort \
        "$ENGINE/tools/asset_pipeline/refpack_native.rs" -o "$ENGINE/target/native/refpack.dll"
    curl -fsSL "$CONVERTER_URL" -o "$ENGINE/iw4l_skate_convert.py"
    "$STATE/venv/bin/python" "$ENGINE/iw4l_skate_convert.py" --xex "$SKATE_DIR/default.xex" --out "$ROOT/skate-data" ||
        die "the Skate 3 conversion failed (see the lines above)" "check that the Skate 3 folder is a complete extraction"
    [ -f "$ROOT/skate-data/assets/private/skater.glb" ] || die "the conversion finished without skater.glb"
fi

# ── Skate 3 sounds ─────────────────────────────────────────────────────────────────────────────
if [ ! -f "$ROOT/skate-audio/pop_1.wav" ]; then
    step "Converting Skate 3 sounds"
    need_skate
    if ! command -v vgmstream-cli >/dev/null; then
        curl -fsSL "$VGMSTREAM_URL" -o "$STATE/vgmstream.zip"
        python3 -c "import zipfile,sys; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])" "$STATE/vgmstream.zip" "$STATE/bin"
        chmod +x "$STATE/bin/vgmstream-cli"
    fi
    "$STATE/venv/bin/python" "$ROOT/tools/skate_audio.py" "$SKATE_DIR/data" "$ROOT/skate-audio" ||
        echo "  (sounds failed; skating still works, just silent)"
fi

# ── Local server ───────────────────────────────────────────────────────────────────────────────
SERVER="$ROOT/server"
# One fixed project name, so the server is the same Docker project whatever the folder is called.
export SKATECRAFT_SERVER_PROJECT=${SKATECRAFT_SERVER_PROJECT:-world-of-skatecraft}
compose() { docker compose -p "$SKATECRAFT_SERVER_PROJECT" --project-directory "$SERVER" -f "$SERVER/compose.yaml" "$@"; }
if [ ! -f "$SERVER/compose.yaml" ]; then
    step "Setting up the local vmangos server"
    [ -d "$SERVER/.git" ] || git clone --quiet "$SERVER_URL" "$SERVER"
    git -C "$SERVER" checkout --quiet "$SERVER_REV"
    cp "$SERVER/config/mangosd.conf.example" "$SERVER/config/mangosd.conf"
    cp "$SERVER/config/realmd.conf.example" "$SERVER/config/realmd.conf"
    TZ_NAME=$(timedatectl show -p Timezone --value 2>/dev/null || cat /etc/timezone 2>/dev/null || echo Etc/UTC)
    sed -e "s#TZ=Etc/UTC#TZ=${TZ_NAME}#" -e "s#user: 1000:1000#user: $(id -u):$(id -g)#" \
        "$SERVER/compose.yaml.example" > "$SERVER/compose.yaml"
fi
if [ ! -d "$SERVER/storage/mangosd/extracted-data/maps" ]; then
    step "Extracting server maps from your WoW client (this takes HOURS; leave it running)"
    docker pull --quiet "$SERVER_IMAGE" >/dev/null
    docker run --rm --user "$(id -u):$(id -g)" \
        -v "$(realpath "$ROOT/WoW")":/opt/vmangos/storage/client-data \
        -v "$SERVER/storage/mangosd/extracted-data":/opt/vmangos/storage/extracted-data \
        "$SERVER_IMAGE" extract-client-data </dev/null ||
        die "map extraction failed (see the lines above)"
fi
step "Building the server with the Skateboarding profession (a few minutes)"
"$ROOT/setup/build-server.sh" "$SERVER" || die "the server build failed (see the lines above)"
step "Starting the server"
compose up -d
printf '  waiting for the world server'
for _ in $(seq 120); do
    [ "$(docker inspect -f '{{.State.Health.Status}}' "$(compose ps -q mangosd)" 2>/dev/null)" = healthy ] && break
    printf '.'; sleep 5
done
echo
[ "$(docker inspect -f '{{.State.Health.Status}}' "$(compose ps -q mangosd)" 2>/dev/null)" = healthy ] ||
    die "the world server did not come up within 10 minutes" "look at: docker compose -p world-of-skatecraft -f server/compose.yaml logs mangosd"

# ── Account ────────────────────────────────────────────────────────────────────────────────────
if [ ! -f "$STATE/account" ]; then
    step "Game account"
    read -r -p "Account name (letters and digits): " ACCOUNT </dev/tty
    read -r -s -p "Password: " PASSWORD </dev/tty; echo
    python3 "$ROOT/tools/make_account.py" "$ACCOUNT" "$PASSWORD" --server "$SERVER" ||
        die "could not create the account"
    echo "$ACCOUNT" > "$STATE/account"
fi

# ── Build ──────────────────────────────────────────────────────────────────────────────────────
step "Building the game (first time: several minutes)"
cargo build --release -p benilla || die "the build failed (see the lines above)" "$(packages_hint)"

step "Done"
echo "  Start the game with:  ./play.sh"
echo "  Log in as $(cat "$STATE/account"), create a character, press J to skate (Xbox controller)."
