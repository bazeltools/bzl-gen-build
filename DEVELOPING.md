# Building

Run this first to build everything.

```bash
./prepare_all_apps.sh
```

# Testing

Run the following to test using the example repo:

```
.github/ci_scripts/integration_test.sh
```

Or export `REPO_PATH` to be the path of the Bazel repo your working on here...

```bash
# Pick jvm or python

ECOSYSTEM=jvm
ECOSYSTEM=python
ECOSYSTEM=protos

cargo run --bin bzl_gen_build_driver --release -- \
  --input-path build_tools/lang_support/create_lang_build_files/bazel_${ECOSYSTEM}_modules.json \
  --working-directory $REPO_PATH \
  --cache-path ~/.cache/bazel_codegen \
  extract \
  --extractor protos:/tmp/bzl-gen-build/protos-entity-extractor \
  --extractor java:/tmp/bzl-gen-build/java-entity-extractor \
  --extractor scala:/tmp/bzl-gen-build/scala-entity-extractor \
  --extractor python:/tmp/bzl-gen-build/python-entity-extractor \
  --extracted-mappings /tmp/extracted_mappings.${ECOSYSTEM}.json

cargo run --bin bzl_gen_build_driver  --release -- \
  --input-path build_tools/lang_support/create_lang_build_files/bazel_${ECOSYSTEM}_modules.json \
  --working-directory $REPO_PATH \
  --cache-path ~/.cache/bazel_codegen \
  extract-defs \
  --extracted-mappings /tmp/extracted_mappings.${ECOSYSTEM}.json \
  --extracted-defs /tmp/extracted_defs.${ECOSYSTEM}.json

cargo run --bin bzl_gen_build_driver --release -- \
  --input-path build_tools/lang_support/create_lang_build_files/bazel_${ECOSYSTEM}_modules.json \
  --working-directory $REPO_PATH \
  --cache-path ~/.cache/bazel_codegen \
  build-graph \
  --extracted-mappings /tmp/extracted_mappings.${ECOSYSTEM}.json \
  --extracted-defs /tmp/extracted_defs.${ECOSYSTEM}.json \
  --graph-out /tmp/graph_data.${ECOSYSTEM}.json

cargo run --bin bzl_gen_build_driver --release -- \
  --input-path build_tools/lang_support/create_lang_build_files/bazel_${ECOSYSTEM}_modules.json \
  --working-directory $REPO_PATH \
  --cache-path ~/.cache/bazel_codegen \
  print-build \
  --graph-data /tmp/graph_data.${ECOSYSTEM}.json
```

Shared API type from languages:

```javascript
    {
        # A remote label @pip_... or local repo relative path src/a/b/c
        "label_or_repo_path": ""

        # List of objects defined in this file/target
        "defs": [],

        # List of references to other things defined in this target
        "refs": [],

        # List of commands that can be supplied to bzl_build_gen
        # These are for special cases
        #
        # `ref` Add a manual reference to the specified def even though it didn't exist in this file
        # `unref` Remove a reference seen in this file
        # `def` Add a manual definition in this file on something that isn't visible to the parsers of the file
        # `undef` Remove a definition as seen in this file
        # `runtime_ref` Add a runtime dependency in this file. This is a colored edge that can be upgraded to a ref if another file or something in the file uses it.
        #
        #    value(SrcDirective::Ref, tag("ref")),
        #    value(SrcDirective::Unref, tag("unref")),
        #    value(SrcDirective::Def, tag("def")),
        #    value(SrcDirective::Undef, tag("undef")),
        #    value(SrcDirective::RuntimeRef, tag("runtime_ref")),
        #    value(SrcDirective::RuntimeUnref, tag("runtime_unref")),
        bzl_gen_build_commands: []
    }
```


# Implications on the rules/macros used in the repo..

We expect to be able to supply the following keys generically:
- `srcs` , list of per language source files
- `deps` , list of dependencies from the graph
- `runtime_deps`, list of dependencies used at runtime, but not compile time. (For some languages like python, a macro wrapping py_library can help merge this into deps)

# Testing on the repo side

```bash
TOOLING_WORKING_DIRECTORY=/tmp/bzl-gen-build ./bazel run build_tools/lang_support/create_lang_build_files:regenerate_jvm_build_files
```
