#!/bin/bash

set -euo pipefail

set +x
BZL_GEN_BUILD_VERSION=v0.1-81
BZL_GEN_BUILD_SOURCE_GITHUB_REPO=bazeltools/bzl-gen-build

function log(){
    DTE="$(date '+%Y %m %d %H:%M:%S')"
    GREEN='\033[0;32m'
    NC='\033[0m'

    printf "\n\n$GREEN$DTE - $1$NC\n\n" 1>&2
}


if [ "$(uname -s)" == "Linux" ]; then
  export BZL_GEN_PLATFORM='linux-ubuntu-20.04'
  export BUILDIFIER_PLATFORM_SUFFIX="-linux-amd64"
elif [ "$(uname -s)" == "Darwin" ]; then
  ARCH="$(uname -m)"
  if [ "$ARCH" == "arm64" ]; then
    export BZL_GEN_PLATFORM='macos-arm64'
    export BUILDIFIER_PLATFORM_SUFFIX="-darwin-arm64"
  else
    export BZL_GEN_PLATFORM='macos-x86'
    export BUILDIFIER_PLATFORM_SUFFIX="-darwin-amd64"
  fi
else
  "Your platform $(uname -s) is unsupported, sorry"
  exit 1
fi

export BZL_GEN_BUILD_CACHE_PATH=/tmp/bzl_gen_build/code

if [ -z "${BZL_GEN_BUILD_TOOLS_PATH:-}" ]; then
  BZL_GEN_BUILD_TOOLS_PATH=$HOME/.cache/bzl_gen_build/tools
fi
mkdir -p "$BZL_GEN_BUILD_TOOLS_PATH"
BZL_BUILD_GEN_TOOLS_URL=https://github.com/${BZL_GEN_BUILD_SOURCE_GITHUB_REPO}/releases/download/${BZL_GEN_BUILD_VERSION}/bzl-gen-build-${BZL_GEN_PLATFORM}.tgz
BZL_BUILD_GEN_TOOLS_SHA_URL=https://github.com/${BZL_GEN_BUILD_SOURCE_GITHUB_REPO}/releases/download/${BZL_GEN_BUILD_VERSION}/bzl-gen-build-${BZL_GEN_PLATFORM}.tgz.sha256
BZL_BUILD_GEN_TOOLS_LOCAL_PATH="${BZL_GEN_BUILD_TOOLS_PATH}/${BZL_GEN_BUILD_VERSION}"


if [ -z "${TOOLING_WORKING_DIRECTORY:-}" ]; then
    if [ ! -d "$BZL_BUILD_GEN_TOOLS_LOCAL_PATH" ]; then
        log "Fetching bzl-gen-build ${BZL_GEN_BUILD_VERSION}"
        rm -rf "$BZL_BUILD_GEN_TOOLS_LOCAL_PATH" &> /dev/null || true
        DOWNLOAD_PATH="${BZL_BUILD_GEN_TOOLS_LOCAL_PATH}.tgz"
        1>&2 fetch_binary "${DOWNLOAD_PATH}" "$BZL_BUILD_GEN_TOOLS_URL" "$BZL_BUILD_GEN_TOOLS_SHA_URL"
        TMP_P=${BZL_BUILD_GEN_TOOLS_LOCAL_PATH}.tmp
        mkdir -p $TMP_P
        cd $TMP_P
        set +e
        tar zxvf $DOWNLOAD_PATH &> /dev/null
        RET=$?
        if [ "$RET" != "0" ]; then
            tar zxvf $DOWNLOAD_PATH 1>&2
        fi
        set -e
        cd $REPO_ROOT
        mv $TMP_P $BZL_BUILD_GEN_TOOLS_LOCAL_PATH
    fi
fi

if [ -z "$GEN_FLAVOR" ]; then
    echo "Need to have specified GEN_FLAVOR before sourcing this"
fi

if [ -n "${INVOKED_VIA_BAZEL:-}" ]; then
    REPO_ROOT="$BUILD_WORKING_DIRECTORY"
else
    REPO_ROOT="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && cd ../../../ && pwd )"
fi


cd $REPO_ROOT

log "Initial bazel command, may sync tools."


set +e
bazel info output_base &> /dev/null
RET=$?
set -e
if [ "$RET" != "0" ]; then
    bazel info output_base
fi

export OUTPUT_BASE="$(bazel info output_base 2> /dev/null)"

if [ -z "$OUTPUT_BASE" ] || [ ! -d "$OUTPUT_BASE" ]; then
    echo "Invalid output base value $OUTPUT_BASE" 1>&2
    bazel info output_base
    exit 1
fi
if [ ! -f WORKSPACE ]; then
    echo "Attempted to get into the bazel root and find the workspace, but we failed, we are in $PWD" 1>&2
    exit 1
fi

export TMP_WORKING_STATE=/tmp/build_gen_${GEN_FLAVOR}
export TMP_DOWNLOAD_CACHE=/tmp/build_downloads

mkdir -p $TMP_DOWNLOAD_CACHE

mkdir -p $BZL_GEN_BUILD_CACHE_PATH
rm -rf $TMP_WORKING_STATE
mkdir -p $TMP_WORKING_STATE
export BZL_BUILD_GEN_EXTERNAL_FILES_PATH=$TMP_WORKING_STATE/external_files
mkdir -p $BZL_BUILD_GEN_EXTERNAL_FILES_PATH

if [ -z "${TOOLING_WORKING_DIRECTORY:-}" ]; then
    export TOOLING_WORKING_DIRECTORY="$BZL_BUILD_GEN_TOOLS_LOCAL_PATH"
else
    export TOOLING_WORKING_DIRECTORY="$TOOLING_WORKING_DIRECTORY"
fi

if [ ! -d "$TOOLING_WORKING_DIRECTORY" ]; then
    echo "$TOOLING_WORKING_DIRECTORY should point at the location of an unpacked bzl_build_gen config"  1>&2
    exit 1
fi

function generate_cache_key() {
    rm -f /tmp/bzl_gen_remote_cache_key_builder &> /dev/null || true
    set -e
    for arg in $@; do
        cat $arg >> /tmp/bzl_gen_remote_cache_key_builder
    done
    echo "$BZL_GEN_BUILD_VERSION" >> /tmp/bzl_gen_remote_cache_key_builder
    shasum -a 256 /tmp/bzl_gen_remote_cache_key_builder | awk '{print $1}'
}

function run_system_apps() {
    CFG="$1"
    if [ -z "$CFG" ]; then
        echo "Must supply a config file, like the bazel_jvm_modules.json" 1>&2
        exit 1
    fi

    if [ ! -f "$CFG" ]; then
        echo "Config argument must be a file, given $CFG" 1>&2
        exit 1
    fi

    set -ex
    ${TOOLING_WORKING_DIRECTORY}/system-driver-app \
        --input-path $CFG \
        --working-directory $REPO_ROOT \
        --cache-path ${BZL_GEN_BUILD_CACHE_PATH} extract \
        --extractor scala:${TOOLING_WORKING_DIRECTORY}/scala-entity-extractor \
        --external-generated-root ${TMP_WORKING_STATE}/external_files \
        --extractor java:${TOOLING_WORKING_DIRECTORY}/java-entity-extractor \
        --extractor python:${TOOLING_WORKING_DIRECTORY}/python-entity-extractor \
        --extracted-mappings ${TMP_WORKING_STATE}/extracted_mappings.json

    ${TOOLING_WORKING_DIRECTORY}/system-driver-app \
        --input-path $CFG \
        --working-directory $REPO_ROOT \
        --cache-path ${BZL_GEN_BUILD_CACHE_PATH} \
        extract-defs \
        --extracted-mappings ${TMP_WORKING_STATE}/extracted_mappings.json \
        --extracted-defs ${TMP_WORKING_STATE}/extracted_defs.json

    ${TOOLING_WORKING_DIRECTORY}/system-driver-app \
        --input-path $CFG \
        --working-directory $REPO_ROOT  \
        --cache-path ${BZL_GEN_BUILD_CACHE_PATH} \
        build-graph \
        --extracted-mappings ${TMP_WORKING_STATE}/extracted_mappings.json \
        --extracted-defs ${TMP_WORKING_STATE}/extracted_defs.json \
        --graph-out ${TMP_WORKING_STATE}/graph_data.json

    ${TOOLING_WORKING_DIRECTORY}/system-driver-app \
        --input-path $CFG \
        --working-directory $REPO_ROOT \
        --cache-path ${BZL_GEN_BUILD_CACHE_PATH} \
        print-build \
        --graph-data ${TMP_WORKING_STATE}/graph_data.json
    set +x
}

cd $REPO_ROOT
