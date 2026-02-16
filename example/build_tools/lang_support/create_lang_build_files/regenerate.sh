#!/usr/bin/env bash

set -o errexit  # abort on nonzero exitstatus
set -o nounset  # abort on unbound variable
set -o pipefail # don't hide errors within pipes

if [ -n "${INVOKED_VIA_BAZEL:-}" ]; then
    REPO_ROOT="$BUILD_WORKING_DIRECTORY"
else
    REPO_ROOT="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && cd ../../../ && pwd )"
fi

source "$REPO_ROOT/build_tools/lang_support/create_lang_build_files/regenerate_protos_build_files.sh"
source "$REPO_ROOT/build_tools/lang_support/create_lang_build_files/regenerate_python_build_files.sh"
source "$REPO_ROOT/build_tools/lang_support/create_lang_build_files/regenerate_jvm_build_files.sh"
