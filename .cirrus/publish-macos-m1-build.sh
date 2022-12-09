#!/bin/bash -e


# export MACOS_ARM64_BUILD=true
brew install java11

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

export PATH=$PATH:$HOME/.cargo/bin

WORKING_DIRECTORY=`pwd`

./.github/ci_scripts/prepare_output.sh bzl-gen-build-macos-arm64.tgz staging-directory


if [[ "$CIRRUS_RELEASE" == "" ]]; then
  echo "Not a release. No need to deploy!"
  exit 0
fi

if [[ "$GITHUB_TOKEN" == "" ]]; then
  echo "Please provide GitHub access token via GITHUB_TOKEN environment variable!"
  exit 1
fi

file_content_type="application/octet-stream"

files_to_upload=(
  bzl-gen-build-macos-arm64.tgz
  bzl-gen-build-macos-arm64.tgz.sha256
)

for fpath in $files_to_upload
do
  echo "Uploading $fpath..."
  name=$(basename "$fpath")
  url_to_upload="https://uploads.github.com/repos/$CIRRUS_REPO_FULL_NAME/releases/$CIRRUS_RELEASE/assets?name=$name"
  curl -X POST \
    --data-binary @$WORKING_DIRECTORY/stagin-directory/$fpath \
    --header "Authorization: token $GITHUB_TOKEN" \
    --header "Content-Type: $file_content_type" \
    $url_to_upload
done