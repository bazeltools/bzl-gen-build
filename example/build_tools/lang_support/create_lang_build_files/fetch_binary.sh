#!/bin/bash

set -o errexit  # abort on nonzero exitstatus
set -o nounset  # abort on unbound variable
set -o pipefail # don't hide errors within pipes
# set -o xtrace

function fetch_binary() {
  TARGET_PATH="$1"
  FETCH_URL="$2"
  URL_SHA="$3"
  set +e

  # Ocassionally GitHub has returned invalid data, even though it wasn't a 500.
  fetch_binary_inner "$TARGET_PATH" "$FETCH_URL" "$URL_SHA"
  RET="$?"
  if [[ "$RET" == "0" ]]; then
    return
  fi
  echo "failed to download from upstream. will retry in 20 seconds"
  sleep 20

  fetch_binary_inner "$TARGET_PATH" "$FETCH_URL" "$URL_SHA"
  RET="$?"
  if [[ "$RET" == "0" ]]; then
    return
  fi
  echo "failed to download from upstream. will retry in 20 seconds"
  sleep 20

  # Final attempt, just pass through the error code from the inner call.
  set -e
  fetch_binary_inner "$TARGET_PATH" "$FETCH_URL" "$URL_SHA"
}

function fetch_binary_inner() {
  RND_UID="${USER}_$(date "+%s")_${RANDOM}_${RANDOM}"
  export BUILD_DIR="${TMPDIR}/bazel_b_${RND_UID}"
  mkdir -p $BUILD_DIR

  TARGET_PATH="$1"
  FETCH_URL="$2"
  URL_SHA="$3"
  set +e
  which shasum &> /dev/null
  HAVE_SHASUM=$?
  set -e
  if [[ ! -f $TARGET_PATH ]]; then
    echo "need to fetch a new copy of tool, fetching... ${FETCH_URL}"
    ( # Opens a subshell
      set -e
      cd $BUILD_DIR

      curl -o tmp_download_file -L $FETCH_URL
      chmod +x tmp_download_file

      if [[ "$HAVE_SHASUM" == "0" ]]; then
        if [[ -n "$URL_SHA" ]]; then
          curl -s --show-error -o tmp_download_file_SHA -L $URL_SHA
          GENERATED_SHA_256=$(shasum -a 256 tmp_download_file | awk '{print $1}')
          if [[ "$GENERATED_SHA_256" != "$(cat tmp_download_file_SHA)" ]]; then
            echo "when working on tool: $TARGET_PATH"
            echo "sha256 does not match, expected: $(cat tmp_download_file_SHA) downloaded from ${URL_SHA}"
            echo "but found $GENERATED_SHA_256"
            echo "probably bad download."
            exit 1
          fi
        fi
      fi

      mv tmp_download_file "$TARGET_PATH"
    )
    rm -rf $BUILD_DIR
  fi
}
