def py_binary(
        name = None,
        binary_refs_value = None,
        owning_library = None,
        entity_path = None,
        visibility = None,
        **kwargs):
    if name == None:
        fail("Need to specify name")
    if owning_library == None:
        fail("Need to specify owning_library")
    if entity_path == None:
        fail("Need to specify entity_path")
    if visibility == None:
        fail("Need to specify visibility")
    idx = entity_path.rindex("/")
    relative_entity_path = entity_path
    if idx >= 0:
        relative_entity_path = entity_path[idx + 1:]

    # buildifier: disable=native-python
    native.py_binary(
        name = name,
        main = relative_entity_path,
        legacy_create_init = 1,
        deps = [
            owning_library,
        ],
        srcs = [
            relative_entity_path,
        ],
        visibility = visibility,
    )
