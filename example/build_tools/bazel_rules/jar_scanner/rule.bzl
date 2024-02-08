# jar_scanner_aspect called by py_build_commands.py is intended for scan
# exactly one JAR file, typically ones exposed by scala_import(...),
# and generates JSON file listing the class names used by bzl-gen-build.

def _jar_scanner_impl(target, ctx):
    label = str(target.label)
    name = target.label.name
    if ((not target.label.workspace_name.startswith("_main~maven~maven")) and
        (not label.endswith("proto_java")) and
        (not label.endswith("proto_scala"))):
        return []

    # Make sure the rule has a srcs attribute.
    out = ctx.actions.declare_file("%s_jar_scanner.json" % (target.label.name))
    files = ctx.rule.files
    all_jars = []

    # For protobuf-generated targets, we end up with multiple JAR files
    # returned by info.files, so here I am manually narrowing down to exactly 1 JAR file.
    if hasattr(files, "deps"):
        info = target[DefaultInfo]

        for jar in info.files.to_list():
            if jar.basename.endswith("-src.jar"):
                None
            elif (jar.basename == "scala-reflect.jar") and (not label.startswith("@@_main~maven~maven//:org_scala_lang__scala_reflect")):
                None
            elif ("scalapb-runtime" in jar.basename) and (not label.startswith("@@_main~maven~maven//:com_thesamet_scalapb_scalapb_runtime")):
                None
            else:
                all_jars.append(jar)

        if len(all_jars) > 1:
            # proto targets returns many JARs so we need to pick them up outselves.
            if name.endswith("_java"):
                name = name[:-5]
            elif name.endswith("_scala"):
                name = name[:-6]
            all_jars0 = [jar for jar in all_jars if name in jar.short_path]

            if len(all_jars0) == 0:
                all_jars = [all_jars[-1]]
            else:
                all_jars = [all_jars0[-1]]
    elif hasattr(files, "jars"):
        # this is a scala_import (it seems.... :|)
        for jar in files.jars:
            if not jar.basename.endswith("-sources.jar"):
                all_jars.append(jar)

    if len(all_jars) != 1:
        fail("%s (%s) has incorrect jars: %s" % (label, name, all_jars))

    short = all_jars[0].short_path
    prefix_len = 0
    if label.startswith("@jvm"):
        len_workspace = len(target.label.workspace_name)
        prefix_len = len_workspace + 1  # workspace + /
    if short.startswith("../"):
        prefix_len = 3 + prefix_len
    relative = short[prefix_len:]

    args = ctx.actions.args()
    args.add("--label")
    args.add(label)
    args.add("--input-jar")
    args.add(all_jars[0])
    args.add("--relative-path")
    args.add(relative)
    args.add("--out")
    args.add(out)
    ctx.actions.run(
        outputs = [out],
        inputs = all_jars,
        executable = ctx.files._jarscanner_exe[0],
        mnemonic = "JarScanner",
        arguments = [args],
    )
    return [OutputGroupInfo(jar_scanner_out = depset([out]))]

jar_scanner_aspect = aspect(
    implementation = _jar_scanner_impl,
    attr_aspects = [],
    attrs = {
        "_jarscanner_exe": attr.label(
            default = Label("@external_build_tooling_gen//:jarscanner"),
            allow_files = True,
            cfg = "host",
        ),
    },
)
