#!/usr/bin/env bash
# Builds the vmangos server image with World of Skatecraft's patch (setup/vmangos/skatecraft.patch:
# the Skateboarding profession) and points a vmangos-deploy checkout's compose.yaml at it.
# setup/setup.sh runs this for server/; for another vmangos-deploy checkout:
#   setup/build-server.sh ~/path/to/vmangos-deploy && docker compose -f ~/path/to/vmangos-deploy/compose.yaml up -d
# The first build compiles vmangos (a few minutes). SKATECRAFT_BUILD_JOBS sets its parallelism
# (default 4, so the PC stays usable).
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
DEPLOY=$(cd "${1:-$ROOT/server}" && pwd)
PATCH="$ROOT/setup/vmangos/skatecraft.patch"
# The vmangos commit the patch is made against.
VMANGOS_REV=0e3ff01e76d4758e8a7c3108b2717cc785ed56fa
JOBS=${SKATECRAFT_BUILD_JOBS:-4}
# GNU sha256sum, or macOS's shasum.
sha256() { if command -v sha256sum >/dev/null; then sha256sum; else shasum -a 256; fi; }
# Tagged by the patch's and the server Dockerfile's contents, so a changed patch or vmangos-deploy
# update builds a new image.
IMAGE="world-of-skatecraft/vmangos-server:5875-$(cat "$PATCH" "$DEPLOY/docker/server/Dockerfile" | sha256 | cut -c1-12)"

[ -f "$DEPLOY/compose.yaml" ] || { echo "no compose.yaml in $DEPLOY"; exit 1; }
# The server Dockerfile needs BuildKit; without buildx this fails at once with a clear message.
export DOCKER_BUILDKIT=1
if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    echo "Building $IMAGE (vmangos $VMANGOS_REV with the Skatecraft patch, $JOBS jobs)"
    CONTEXT=$(mktemp -d)
    trap 'rm -rf "$CONTEXT"' EXIT
    cp -r "$DEPLOY/docker" "$CONTEXT/"
    cp "$PATCH" "$CONTEXT/docker/patches/zz-skatecraft.patch"
    sed "s/make -j\"\$(nproc)\"/make -j$JOBS/" "$CONTEXT/docker/server/Dockerfile" > "$CONTEXT/Dockerfile.skatecraft"
    docker build -t "$IMAGE" -f "$CONTEXT/Dockerfile.skatecraft" \
        --build-arg VMANGOS_REVISION="$VMANGOS_REV" \
        --build-arg VMANGOS_CLIENT_VERSION=5875 \
        --build-arg VMANGOS_FAIL_ON_PATCH_ERROR=1 \
        "$CONTEXT"
fi
# Through a temporary file: GNU and BSD sed spell in-place editing differently.
sed -E "s#image: (&mangosd-image )?(ghcr\.io/mserajnik/vmangos-server[:@][^[:space:]]+|world-of-skatecraft/vmangos-server:[^[:space:]]+)#image: \1$IMAGE#" \
    "$DEPLOY/compose.yaml" > "$DEPLOY/compose.yaml.new"
mv "$DEPLOY/compose.yaml.new" "$DEPLOY/compose.yaml"
echo "$DEPLOY/compose.yaml now runs $IMAGE"
