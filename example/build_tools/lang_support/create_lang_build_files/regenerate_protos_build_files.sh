#!/usr/bin/env bash

set -o errexit  # abort on nonzero exitstatus
set -o nounset  # abort on unbound variable
set -o pipefail # don't hide errors within pipes

if [ -n "${INVOKED_VIA_BAZEL:-}" ]; then
    REPO_ROOT="$BUILD_WORKING_DIRECTORY"
else
    REPO_ROOT="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && cd ../../../ && pwd )"
fi

GEN_FLAVOR=protos
source "$REPO_ROOT/build_tools/lang_support/create_lang_build_files/bzl_gen_build_common.sh"

log "generate core build files ($GEN_FLAVOR)"

run_system_apps "build_tools/lang_support/create_lang_build_files/bazel_${GEN_FLAVOR}_modules.json"
