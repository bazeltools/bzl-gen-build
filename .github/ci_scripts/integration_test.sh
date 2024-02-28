#!/usr/bin/env bash

set -o errexit  # abort on nonzero exitstatus
set -o nounset  # abort on unbound variable
set -o pipefail # don't hide errors within pipes

cd example
TOOLING_WORKING_DIRECTORY=/tmp/bzl-gen-build source build_tools/lang_support/create_lang_build_files/regenerate.sh
bazel test ...

changes=$(git diff --name-only --diff-filter=ACMRT | xargs)
if [ ! -z "$changes" ]; then
    echo "::error file=example/WORKSPACE::Generated $changes differs from the checked-in version"
    git diff --exit-code
    exit 1
fi
