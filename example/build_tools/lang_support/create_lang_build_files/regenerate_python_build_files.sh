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

bazel query '@pip//...' | grep "@pip.*:pkg" > $TMP_WORKING_STATE/external_targets

bazel query 'kind("py_proto_library", com/...)' > /dev/null
cat $OUTPUT_BASE/command.log | grep '//' >> $TMP_WORKING_STATE/external_targets

CACHE_KEY="$(generate_cache_key $TMP_WORKING_STATE/external_targets $REPO_ROOT/WORKSPACE $REPO_ROOT/requirements_lock_3_10.txt)"
rm -rf $TMP_WORKING_STATE/external_files &> /dev/null || true
# try_fetch_from_remote_cache "remote_python_${CACHE_KEY}"

# if [ ! -d $TMP_WORKING_STATE/external_files ]; then
    # log "cache wasn't ready or populated"
    bazel run build_tools/bazel_rules/wheel_scanner:py_build_commands -- $TMP_WORKING_STATE/external_targets $TMP_WORKING_STATE/external_targets_commands.sh
    chmod +x "${TMP_WORKING_STATE}/external_targets_commands.sh"
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
  --overwrite PY

# If using WORKSPACE with pip_parse, you may need to set incompatible_generate_aliases = True.
log "Rewriting python targets to --incompatible_generate_aliases form; @@rules_python~0.24.0~pip~pip_39_pandas// to @pip//pandas"

find . -name BUILD.bazel \
  -exec sed -i.bak 's,"@@rules_python~0.24.0~pip~pip_39_\([a-zA-Z0-9_]*\)//:pkg","@pip//\1",' {} \; \
  -exec rm {}.bak \;
