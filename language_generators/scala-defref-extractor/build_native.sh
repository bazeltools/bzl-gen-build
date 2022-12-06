#!/bin/bash

set -e

SCRIPTS_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd $SCRIPTS_DIR

OUTPUT_DIR="${OUTPUT_DIR:-/tmp}"
if [ ! -d "$OUTPUT_DIR" ]; then
    mkdir -p $OUTPUT_DIR
fi

./sbt clean
./sbt '; set mainClass in Compile := Some("io.bazeltools.buildgen.scaladefref.Main"); nativeImage' 1>&2
rm -f ${OUTPUT_DIR}/scala-entity-extractor
cp target/native-image/scala-defref-extractor ${OUTPUT_DIR}/scala-entity-extractor

./sbt clean
./sbt '; set mainClass in Compile := Some("io.bazeltools.buildgen.javadefref.Main"); nativeImage' 1>&2
rm -f ${OUTPUT_DIR}/java-entity-extractor
cp target/native-image/scala-defref-extractor ${OUTPUT_DIR}/java-entity-extractor


echo "Emitted ${OUTPUT_DIR}/scala-entity-extractor"
echo "Emitted ${OUTPUT_DIR}/java-entity-extractor"