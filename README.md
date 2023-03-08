# Modulular build generator

This is a modular build generator, its designed so its easy to pick and choose components, modify intermediate states (all json). post process or use the outputs for new purposes as it makes sense.


## Extractors
These run against target languages to generate a set of:
- classes/entities defined in a given language file(or files).
- classes/entities referred to by a given language file(or files).
- Inline directives in that languages comment format to be expressed to the system. (more details below on the directives)

## Extractors supported
So far we have support for:
- Scala
- Java
- Python

We will likely add pretty soon:
- Protobuf


## System driver
This is a an application that runs in multiple modes to try connect together phases of the pipeline. You can run some, massage/edit/change the data and run more as it makes sense.

### System driver: extract
This mode is to prepare the inputs to the system, it will run + cache the outputs of using the extractors mentioned above to pull out the definitions, references and directives. It can also optionally take a set of generated external files already built of this format - this is mostly used to account for running an external system to figure out 3rdparty defintions/references. (In bazel, this often would be an aspect).

### System driver: extract-defs
This is a relatively simple app, and maybe should be eliminated in future. But its goal is to take teh outputs from `extract` and trim to a smaller number (collapsing up a tree) of files containing just definitions. We do this so in future phases when we need to load everything we can get all our definitions first to trim out all the files as they are being loaded. Scala/java can have a lot of references as they are often heuristic based when we have limited insights (wildcard imports).

### System driver: build-graph
This system is to resolve all of the links between the graph. This will collapse nodes together which have circular dependencies between them to a common ancestor. The output will contain all of the final nodes, along with which sets of source nodes were collapsed into them, and their dependencies.

### System driver: print-build
This will print out all of the build files, preforming any last application of directives as necessary



## Directives
We support both in the configuration files and inline a few flavors of Directives

## Directives: Source directives
These are applied locally to the files they are applied against. These can alter the outcome/behavior of what the `extract` command above will have produced into the system.
- `ref`, This adds a reference as if the current file referred to this entity
- `unref`, Remove a reference from this file, the extractor might believe this file depends on this reference, but filter it out
- `def`, Add a new definition that we can forcibly say comes from this file. Using this can either manually or via another tool allow for the production of new types by either scala macros or java plugins.
- `undef`, Remove a definition from this file so it won't be seen as coming from here in the graph
- `runtime_ref`, Add a new runtime definition, since things only needed at runtime cannot usually be seen from the source code these can help indicate types/sources needed to run this in tests/deployment.
- `runtime_unref`, the dual of the above, though generally not really used often

## Directives: Entity directives
These are used to try build extra links into the chain of dependencies.
- `link`, This has the form of connecting one entity to several others. That is if target `A` depends on `com.foo.Bar`, and a link exists connecting `com.foo.Bar` to `com.animal.Cat, com.animal.Dog`. Then when we see `com.foo.Bar` as a dependency of any target, such as `A`, it will act as if it also depends on `Cat` and `Dog.

## Directives: Manual reference directive
These directives are used as late applying commands, they will alter the final printed build file, but not be considered in graph resolution.
- `manual_runtime_ref`, add a runtime dependency on the _target_ given. That is, not an entity but an actual target addressable in the build.
- `manual_ref`, add a compile time dependency on the _target_ given. That is, not an entity but an actual target addressable in the build.

## Directives: Binary reference directive
Today there is only a single form of this, though more though probably should go into this. And if it should merge with the manual directives above. This is used to generate binary targets.
- `binary_generate: binary_name[@ target_value]`  , This will generate a binary called `binary_name`, and optionally we pass in some information (such as a jvm class name), to the rule that generates the binary.
