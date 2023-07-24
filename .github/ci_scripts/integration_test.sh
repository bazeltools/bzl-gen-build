#!/usr/bin/env bash

set -o errexit  # abort on nonzero exitstatus
set -o nounset  # abort on unbound variable
set -o pipefail # don't hide errors within pipes

cd example
# TOOLING_WORKING_DIRECTORY=/tmp/bzl-gen-build source build_tools/lang_support/create_lang_build_files/regenerate_protos_build_files.sh
TOOLING_WORKING_DIRECTORY=/tmp/bzl-gen-build source build_tools/lang_support/create_lang_build_files/regenerate_python_build_files.sh
bazel test ...
