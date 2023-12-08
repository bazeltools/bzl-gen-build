#!/usr/bin/env bash

set -o errexit  # abort on nonzero exitstatus
set -o nounset  # abort on unbound variable
set -o pipefail # don't hide errors within pipes

if [ -n "${INVOKED_VIA_BAZEL:-}" ]; then
    REPO_ROOT="$BUILD_WORKING_DIRECTORY"
else
    REPO_ROOT="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && cd ../../../ && pwd )"
fi

GEN_FLAVOR=python
source "$REPO_ROOT/build_tools/lang_support/create_lang_build_files/bzl_gen_build_common.sh"
set -x

bazel query '@pip//...' | grep '@pip' > $TMP_WORKING_STATE/external_targets

# try_fetch_from_remote_cache "remote_python_${CACHE_KEY}"

# if [ ! -d $TMP_WORKING_STATE/external_files ]; then
    # log "cache wasn't ready or populated"
    bazel run build_tools/bazel_rules/wheel_scanner:py_build_commands -- $TMP_WORKING_STATE/external_targets $TMP_WORKING_STATE/external_targets_commands.sh
    chmod +x ${TMP_WORKING_STATE}/external_targets_commands.sh
    mkdir -p $TMP_WORKING_STATE/external_files
    if [[ -d $TOOLING_WORKING_DIRECTORY ]]; then
        BZL_GEN_BUILD_TOOLS_PATH=$TOOLING_WORKING_DIRECTORY ${TMP_WORKING_STATE}/external_targets_commands.sh
    else
        BZL_GEN_BUILD_TOOLS_PATH=$BZL_BUILD_GEN_TOOLS_LOCAL_PATH ${TMP_WORKING_STATE}/external_targets_commands.sh
    fi

    # update_remote_cache "remote_python_${CACHE_KEY}"
# fi

run_system_apps "build_tools/lang_support/create_lang_build_files/bazel_${GEN_FLAVOR}_modules.json" \
  --no-aggregate-source \
  --append
