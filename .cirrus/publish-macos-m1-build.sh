#!/bin/bash -e

brew install java11
sudo ln -sfn /opt/homebrew/opt/openjdk@11/libexec/openjdk.jdk /Library/Java/JavaVirtualMachines/openjdk-11.jdk
export PATH="/opt/homebrew/opt/openjdk@11/bin:$PATH"
export CPPFLAGS="-I/opt/homebrew/opt/openjdk@11/include"
export JAVA_HOME="/opt/homebrew/opt/openjdk@11"
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
  $WORKING_DIRECTORY/staging-directory/bzl-gen-build-macos-arm64.tgz.sha256
  $WORKING_DIRECTORY/staging-directory/bzl-gen-build-macos-arm64.tgz
)

for fpath in "${files_to_upload[@]}"
do
  echo "Uploading $fpath..."
  name=$(basename "$fpath")
  url_to_upload="https://uploads.github.com/repos/$CIRRUS_REPO_FULL_NAME/releases/$CIRRUS_RELEASE/assets?name=$name"
  curl -v --fail -X POST \
    --data-binary @$fpath \
    --header "Authorization: token $GITHUB_TOKEN" \
    --header "Content-Type: $file_content_type" \
    $url_to_upload
done
