bzl-gen-build
=============

**bzl-gen-build** is a polyglot modular BUILD generator for Bazel, implemented in Rust.
It is designed so it is easy to pick and choose components, modify intermediate states (all JSON), and post-process or use the outputs for new purposes as it makes sense.

bzl-gen-build works by running the following phases for each file types:
1. Extract classes/entities defined in source files and 3rdparty binaries
2. Extract `import` statements and bzl-gen-build directives in comments
3. Build graph of usages and definition sites
4. Generate Bazel targets

### Extractors supported
So far we have support for:
- Scala
- Java
- Python
- Protobuf

Usage
-----

See `example/` directory for the complete setup.

```bash
./build_tools/lang_support/create_lang_build_files/delete_build_files.sh
./build_tools/lang_support/create_lang_build_files/regenerate_protos_build_files.sh
./build_tools/lang_support/create_lang_build_files/regenerate_python_build_files.sh
bazel test ...
```

These scripts are calling:

```bash
GEN_FLAVOR=protos
source "$REPO_ROOT/build_tools/lang_support/create_lang_build_files/bzl_gen_build_common.sh"
run_system_apps "build_tools/lang_support/create_lang_build_files/bazel_${GEN_FLAVOR}_modules.json" \
  --no-aggregate-source \
  --append
```

The script is setup to download bzl-gen-build from GitHub and run the bzl-gen-build `system-driver-app` with the appropriate commands.

Directives
----------
Sometimes we need to help out the bzl-gen-build by supplying directives.
For example you might need additional dependencies for tests or macros.
We support a few flavors of directives, both in the Module Configuration files and inline in the source code.

```scala
// bzl_gen_build:runtime_ref:com.example.something.DefaultSomethingConfiguration
class SomeTest {
}
```

## Directives: Source directives
These are applied locally to the files they are applied against. These can alter the outcome/behavior of what the `extract` command above will have produced into the system.
- `ref`, This adds a reference as if the current file referred to this entity
- `unref`, Remove a reference from this file, the extractor might believe this file depends on this reference, but filter it out
- `def`, Add a new definition that we can forcibly say comes from this file. Using this can either manually or via another tool allow for the production of new types by either scala macros or java plugins.
- `undef`, Remove a definition from this file so it won't be seen as coming from here in the graph
- `runtime_ref`, Add a new runtime definition, since things only needed at runtime cannot usually be seen from the source code these can help indicate types/sources needed to run this in tests/deployment.
- `runtime_unref`, the dual of the above, though generally not really used often

## Directives: Entity directives
These are used to try to build extra links into the chain of dependencies.
- `link`, This has the form of connecting one entity to several others. That is if target `A` depends on `com.foo.Bar`, and a link exists connecting `com.foo.Bar` to `com.animal.Cat, com.animal.Dog`. Then when we see `com.foo.Bar` as a dependency of any target, such as `A`, it will act as if it also depends on `Cat` and `Dog.

## Directives: Manual reference directive
These directives are used as late applying commands, they will alter the final printed build file, but not be considered in graph resolution.
- `manual_runtime_ref`, add a runtime dependency on the _target_ given. That is, not an entity but an actual target addressable in the build.
- `manual_ref`, add a compile-time dependency on the _target_ given. That is, not an entity but an actual target addressable in the build.

## Directives: Binary reference directive
Today there is only a single form of this, though more though probably should go into this. And if it should merge with the manual directives above. This is used to generate binary targets.
- `binary_generate: binary_name[@ target_value]`, This will generate a binary called `binary_name`, and optionally we pass in some information (such as a jvm class name), to the rule that generates the binary.

Modules
-------

### Extractors
These run against target languages to generate a set of:
- classes/entities defined in a given language file(or files).
- classes/entities referred to by a given language file(or files).
- Inline directives in that language's comment format to be expressed to the system. (more details below on the directives)

### System driver
This is an application that runs in multiple modes to try to connect together phases of the pipeline. You can run some, massage/edit/change the data, and run more as it makes sense.

#### System driver: extract
This mode is to prepare the inputs to the system, it will run + cache the outputs of using the extractors mentioned above to pull out the definitions, references, and directives. It can also optionally take a set of generated external files already built of this format - this is mostly used to account for running an external system to figure out 3rdparty definitions/references. (In Bazel, this often would be an aspect).

#### System driver: extract-defs
This is a relatively simple app and maybe should be eliminated in the future. But its goal is to take the outputs from `extract` and trim to a smaller number (collapsing up a tree) of files containing just definitions. We do this so in future phases when we need to load everything we can get all our definitions first to trim out all the files as they are being loaded. Scala/Java can have a lot of references as they are often heuristic-based when we have limited insights (wildcard imports).

#### System driver: build-graph
This system is to resolve all of the links between the graph. This will collapse nodes together which have circular dependencies between them to a common ancestor. The output will contain all of the final nodes, along with which sets of source nodes were collapsed into them, and their dependencies.

#### System driver: print-build
This will print out all of the build files, performing any last application of directives as necessary

Setup
-----

### Module Config

Each target language is configured using a JSON file, for example `build_tools/lang_support/create_lang_build_files/bazel_python_modules.json`:

```json
{
  "configurations": {
    "python": {
      "file_extensions": ["py"],
      "build_config": {
        "main": {
          "headers": [],
          "function_name": "py_library",
          "target_name_strategy": "source_file_stem"
        },
        "test": {
          "headers": [],
          "function_name": "py_test",
          "target_name_strategy": "source_file_stem"
        }
      },
      "main_roots": ["src/main/python"],
      "test_roots": ["src/test/python"],
      "path_directives": []
    }
  }
}
```

The above will parse all `*.py` files under `src/main/python/` and `src/test/python/` and generate targets under the directories.

#### Secondary rules

In some situations, like for Protocol Buffer schemas, we want to generate secondary rules per each primary rules. This can be configured as follows:

```json
{
  "configurations": {
    "protos": {
      "file_extensions": [
        "proto"
      ],
      "build_config": {
        "main": {
          "headers": [
            {
              "load_from": "@rules_proto//proto:defs.bzl",
              "load_value": "proto_library"
            }
          ],
          "function_name": "proto_library"
        },
        "secondary_rules": {
          "java": {
            "headers": [],
            "function_name": "java_proto_library",
            "extra_key_to_list": {
              "deps": [":${name}"]
            }
          },
          "py": {
            "headers": [
              {
                "load_from": "@rules_python//python:proto.bzl",
                "load_value": "py_proto_library"
              }
            ],
            "function_name": "py_proto_library",
            "extra_key_to_list": {
              "deps": [":${name}"]
            }
          }
        }
      },
      "main_roots": ["com"],
      "test_roots": [],
      "path_directives": []
    }
  }
}
```

### Heuristics

Wildcard imports in Java and Scala can be expensive to resolve, since every subsequent import migth be relative to the previous wildcard.

As an optimization, setting the environment variable `BZL_GEN_SPECIAL_TLDS` to a comma-separated list will tell the driver program to assume any import rooted at one of those names is not relative. For example:

```bash
export BZL_GEN_SPECIAL_TLDS=com,net,org
```

### Extracting definitions from 3rdparty libraries

Extracting definitions from 3rdparty libraries require some setup, also demonstrated in the `example/` repo.

```bash
GEN_FLAVOR=python
source "$REPO_ROOT/build_tools/lang_support/create_lang_build_files/bzl_gen_build_common.sh"
set -x

bazel query '@pip//...' | grep '@pip' > $TMP_WORKING_STATE/external_targets

CACHE_KEY="$(generate_cache_key $TMP_WORKING_STATE/external_targets $REPO_ROOT/WORKSPACE $REPO_ROOT/requirements_lock_3_9.txt)"
rm -rf $TMP_WORKING_STATE/external_files &> /dev/null || true
# try_fetch_from_remote_cache "remote_python_${CACHE_KEY}"

# if [ ! -d $TMP_WORKING_STATE/external_files ]; then
    # log "cache wasn't ready or populated"
    bazel run build_tools/bazel_rules/wheel_scanner:py_build_commands -- $TMP_WORKING_STATE/external_targets $TMP_WORKING_STATE/external_targets_commands.sh
    chmod +x ${TMP_WORKING_STATE}/external_targets_commands.sh
    mkdir -p $TMP_WORKING_STATE/external_files
    if [[ -d $TOOLING_WORKING_DIRECTORY ]]; then
        BZL_GEN_BUILD_TOOLS_PATH=$TOOLING_WORKING_DIRECTORY ${TMP_WORKING_STATE}/external_targets_commands.sh
    else
        BZL_GEN_BUILD_TOOLS_PATH=$BZL_BUILD_GEN_TOOLS_LOCAL_PATH ${TMP_WORKING_STATE}/external_targets_commands.sh
    fi

    # update_remote_cache "remote_python_${CACHE_KEY}"
# fi
```

The above calls `py_build_commands` to generate a Bash script, which then runs wheel_sanner on all the PIP libraries, and generates JSON files in a temp directory. In an actual usage, you would cache these based on the `$CACHE_KEY` so this is done once per dependency update.
