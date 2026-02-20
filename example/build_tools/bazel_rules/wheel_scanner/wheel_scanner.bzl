def _wheel_scanner_impl(target, ctx):
    # Skip type stub targets - they're metadata for type checkers, not runtime code
    if target.label.name.endswith("__pyi"):
        return [OutputGroupInfo(wheel_scanner_out = depset([]))]

    # Make sure the rule has a srcs attribute.
    out_content = ctx.actions.declare_file("%s_wheel_scanner.json" % (target.label.name))

    files = ctx.rule.files
    all_py_files = []
    all_py_relative_paths = []
    site_packages_dir = None  # From first file; only include files under this dir so short paths (e.g. pandas/__init__.py) open correctly
    workspace_root = target.label.workspace_root
    if hasattr(files, "srcs"):
        for file in files.srcs:
            basename = file.basename

            # We don't expect to have dependencies on tests
            if not basename.startswith("rules_python") and not basename.startswith("test_"):
                path = file.path
                if not (workspace_root and "rules_python" in workspace_root and "site-packages/" in path):
                    # Non-rules_python or no site-packages: pass full path, no filtering
                    if workspace_root and workspace_root in path:
                        path_to_pass = path[path.find(workspace_root) + len(workspace_root) + 1:]
                    elif path.startswith(workspace_root):
                        path_to_pass = path[len(workspace_root) + 1:]
                    else:
                        path_to_pass = file.short_path
                    all_py_relative_paths.append(path_to_pass)
                    all_py_files.append(file)
                    continue
                # Use first file to get site-packages dir; only include files under that dir (e.g. pandas/__init__.py).
                if site_packages_dir == None:
                    site_packages_dir = path[:path.find("site-packages/") + len("site-packages")]
                if not path.startswith(site_packages_dir + "/"):
                    continue  # Ignore files not under site_packages_dir
                path_to_pass = path[len(site_packages_dir) + 1:]
                all_py_relative_paths.append(path_to_pass)
                all_py_files.append(file)
    elif (ctx.rule.kind in ["py_proto_library", "py_library"]):
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
    args.add("--label-or-repo-path")
    args.add(str(target.label))

    # So defs are pandas._config.dates not external.rules_python++pip+....site-packages.pandas._config.dates
    if workspace_root and "rules_python" in workspace_root:
        args.add("--import-path-relative-from")
        args.add(workspace_root + "/site-packages/")

    # For rules_python with short paths, use site_packages_dir from first file so tool can open pandas/__init__.py.
    args.add("--working-directory")
    if site_packages_dir != None:
        args.add(site_packages_dir)
    elif workspace_root and "rules_python" in workspace_root:
        args.add(".")
    else:
        args.add(workspace_root if workspace_root else ".")
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
        execution_requirements = {"no-sandbox": "1"},
    )

    return [OutputGroupInfo(wheel_scanner_out = depset([out_content]))]

wheel_scanner_aspect = aspect(
    implementation = _wheel_scanner_impl,
    attr_aspects = [],
    attrs = {
        "_py_exe": attr.label(
            default = Label("@external_build_tooling_gen//:python-entity-extractor"),
            allow_files = True,
            cfg = "exec",
        ),
    },
)
