#!/bin/bash -ex

if [ -n "$MACOS_ARM64_BUILD" ]; then
    export SDKROOT=$(xcrun -sdk macosx --show-sdk-path)
    export MACOSX_DEPLOYMENT_TARGET=$(xcrun -sdk macosx --show-sdk-platform-version)
    export CARGO_EXTRA_ARGS="--target=aarch64-apple-darwin"
    export RUST_TARGET_DIR="target/aarch64-apple-darwin/release"
else
    export CARGO_EXTRA_ARGS=""
    export RUST_TARGET_DIR="target/release"
fi

SCRIPTS_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

cd $SCRIPTS_DIR

if [ -z "$PREPARE_ALL_OUTPUT_DIR" ]; then
 #   rm -rf /tmp/bzl-gen-build
    export PREPARE_ALL_OUTPUT_DIR="/tmp/bzl-gen-build"
fi

if [ ! -d "$PREPARE_ALL_OUTPUT_DIR" ]; then
    mkdir -p $PREPARE_ALL_OUTPUT_DIR
fi

echo "Running scala and python generator building"
#OUTPUT_DIR=$PREPARE_ALL_OUTPUT_DIR language_generators/scala-defref-extractor/build_native.sh

cd crates
cargo build $CARGO_EXTRA_ARGS --release

rm -f  $PREPARE_ALL_OUTPUT_DIR/system-driver-app || true
cp ${RUST_TARGET_DIR}/bzl_gen_build_driver $PREPARE_ALL_OUTPUT_DIR/system-driver-app

rm -f  $PREPARE_ALL_OUTPUT_DIR/python-entity-extractor || true
cp ${RUST_TARGET_DIR}/bzl_gen_python_extractor $PREPARE_ALL_OUTPUT_DIR/python-entity-extractor

echo "workspace(name = \"external_build_tooling_gen\")" > $PREPARE_ALL_OUTPUT_DIR/WORKSPACE

cat > $PREPARE_ALL_OUTPUT_DIR/BUILD.bazel <<EOF
exports_files(["WORKSPACE", "python-entity-extractor"])
EOF
