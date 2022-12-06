set -ex

#!/usr/bin/env bash

OUTPUT_NAME=$1
export OUTPUT_PATH="`pwd`/$2"

if [ ! -d $OUTPUT_PATH ]; then
    mkdir $OUTPUT_PATH
fi

export PREPARE_ALL_OUTPUT_DIR="/tmp/built"
mkdir -p $PREPARE_ALL_OUTPUT_DIR
./prepare_all_apps.sh

ORIGINAL_PWD=`pwd`

cd /tmp/built
tar -czvvf $OUTPUT_PATH/$OUTPUT_NAME *
GENERATED_SHA_256=$(shasum -a 256 $OUTPUT_PATH/$OUTPUT_NAME | awk '{print $1}')
echo $GENERATED_SHA_256 > $OUTPUT_PATH/${OUTPUT_NAME}.sha256
