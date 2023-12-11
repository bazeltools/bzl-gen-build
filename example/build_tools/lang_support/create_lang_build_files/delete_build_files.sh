#!/usr/bin/env bash

set -o errexit  # abort on nonzero exitstatus
set -o nounset  # abort on unbound variable
set -o pipefail # don't hide errors within pipes

if [ -n "${INVOKED_VIA_BAZEL:-}" ]; then
    REPO_ROOT="$BUILD_WORKING_DIRECTORY"
else
    REPO_ROOT="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && cd ../../../ && pwd )"
fi

find "$REPO_ROOT/com" -type f -name "*.bazel" -exec rm -f {} \;
find "$REPO_ROOT/tests" -type f -name "*.bazel" -exec rm -f {} \;
