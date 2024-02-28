def _wheel_scanner_impl(target, ctx):
    # Make sure the rule has a srcs attribute.
    out_content = ctx.actions.declare_file("%s_wheel_scanner.json" % (target.label.name))

    files = ctx.rule.files
    all_py_files = []
    all_py_relative_paths = []
    workspace_root = target.label.workspace_root
    if hasattr(files, "srcs"):
        for file in files.srcs:
            basename = file.basename

            # We don't expect to have dependencies on tests
            # These generated rules python files seem to have some invalid python syntax too
            if not basename.startswith("rules_python_wheel_") and not basename.startswith("test_"):
                path = file.path
                if not path.startswith(workspace_root):
                    fail("Didn't have workspace prefix")
                all_py_relative_paths.append(path[len(workspace_root) + 1:])
                all_py_files.append(file)
    elif ctx.rule.kind == "py_proto_library":
        info = target[DefaultInfo]
        last_file = info.files.to_list()[-1]
        parts = last_file.path.split("/bin/", 1)
        workspace_root = "./{}/bin".format(parts[0])
        relative_path = parts[1]
        all_py_relative_paths.append(relative_path)
        all_py_files.append(last_file)

    input_files = ctx.actions.declare_file("%s_wheel_scanner_input_files.txt" % (target.label.name))
    ctx.actions.write(
        input_files,
        "\n".join(all_py_relative_paths),
    )

    args = ctx.actions.args()
    args.add("--disable-ref-generation")
    args.add("--label-or-repo-path")
    args.add(str(target.label))

    if workspace_root != "":
        args.add("--import-path-relative-from")
        args.add("%s/" % (workspace_root))
    args.add("--working-directory")
    args.add(workspace_root)
    args.add("--relative-input-paths")
    args.add("@%s" % input_files.path)
    args.add("--output")
    args.add(out_content)

    inputs = [input_files]
    inputs.extend(all_py_files)
    ctx.actions.run(
        outputs = [out_content],
        inputs = inputs,
        executable = ctx.files._py_exe[0],
        mnemonic = "WheelScanner",
        arguments = [args],
    )

    return [OutputGroupInfo(wheel_scanner_out = depset([out_content]))]

wheel_scanner_aspect = aspect(
    implementation = _wheel_scanner_impl,
    attr_aspects = [],
    attrs = {
        "_py_exe": attr.label(
            default = Label("@external_build_tooling_gen//:python-entity-extractor"),
            allow_files = True,
            cfg = "host",
        ),
    },
)
