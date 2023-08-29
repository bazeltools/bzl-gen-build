#!/bin/bash

RESULTS_FILE="/tmp/timings_and_commits.txt"

SCALA_VERSIONS=("2.11.12" "2.12.18" "2.13.11")
SCALA_URLS=(
  "https://repo1.maven.org/maven2/org/scala-lang/scala-library/2.11.12/scala-library-2.11.12.jar"
  "https://repo1.maven.org/maven2/org/scala-lang/scala-library/2.12.18/scala-library-2.12.18.jar"
  "https://repo1.maven.org/maven2/org/scala-lang/scala-library/2.13.11/scala-library-2.13.11.jar"
)

for i in "${!SCALA_VERSIONS[@]}"; do
  version="${SCALA_VERSIONS[$i]}"
  URL="${SCALA_URLS[$i]}"
  TARGET="/tmp/scala-library-${version}.jar"

  if [ -f "$TARGET" ]; then
    echo "File $TARGET already exists, skipping download."
    continue
  fi

  echo "Downloading .jar file for Scala ${version}..."
  curl -o "$TARGET" "$URL"
  if [ $? -ne 0 ]; then
    echo "Failed to download .jar file for Scala ${version}."
    exit 1
  fi
done

# Define the Rust project directory
RUST_PROJECT_DIR="./crates"

# Build the Rust project
echo "Building Rust project..."
cd $RUST_PROJECT_DIR
cargo build
if [ $? -ne 0 ]; then
  echo "Failed to build Rust project."
  exit 1
fi

# Define the number of times to run the tool
N=50
TEMP_FILE=$(mktemp)

# Run the executable on the .jar file N times and collect timing information
echo "Scanning the 2.11, 2.12, and 2.13 jars with jarscanner $N times..."
/usr/bin/time -o $TEMP_FILE bash -c '
for i in $(seq 1 '"$N"'); do
  ./target/release/bzl_gen_jarscanner --input-jar /tmp/scala-library-2.11.12.jar --out "/tmp/scala-library-2.11.12.json" --label "benchmark_2.11.12_label" --relative-path /tmp/scala-library-2.11.12.jar
  ./target/release/bzl_gen_jarscanner --input-jar /tmp/scala-library-2.12.18.jar --out "/tmp/scala-library-2.12.18.json" --label "benchmark_2.12.18_label" --relative-path /tmp/scala-library-2.12.18.jar
  ./target/release/bzl_gen_jarscanner --input-jar /tmp/scala-library-2.13.11.jar --out "/tmp/scala-library-2.13.11.json" --label "benchmark_2.13.11_label" --relative-path /tmp/scala-library-2.13.11.jar
done
'

# Get the current git commit hash
GIT_COMMIT=$(git rev-parse HEAD)

# Append timing information and git commit to the results file
echo "---- Timing for $N runs ----" >> $RESULTS_FILE
cat $TEMP_FILE >> $RESULTS_FILE
echo "Git Commit: $GIT_COMMIT" >> $RESULTS_FILE
echo "---------------------------" >> $RESULTS_FILE

# Remove the temporary file
rm $TEMP_FILE
