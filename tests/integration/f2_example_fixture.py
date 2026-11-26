#!/usr/bin/env python3
"""Cópia isolada do blank-desktop para harnesses que editam o exemplo."""
import json
import pathlib
import re
import shutil


def copy_blank(root, fixture, single_output=True):
    source = pathlib.Path(root) / "examples/blank-desktop"
    target = pathlib.Path(fixture)
    shutil.copytree(
        source, target, dirs_exist_ok=True,
        ignore=shutil.ignore_patterns("node_modules"),
    )
    node_modules = source / "node_modules"
    if node_modules.exists() and not (target / "node_modules").exists():
        (target / "node_modules").symlink_to(node_modules, target_is_directory=True)
    prefix = json.dumps(str(pathlib.Path(root) / "packages"))[:-1] + "/"
    for path in target.iterdir():
        if path.suffix not in (".ts", ".tsx", ".js", ".json"):
            continue
        path.write_text(path.read_text().replace('"../../packages/', prefix))
    config = target / "desktop.config.tsx"
    if single_output:
        config.write_text(re.sub(
            r'\n    \{\n      kind: "Panel",\n      id: "blank-panel-output-1",[\s\S]*?\n    \},',
            "", config.read_text(),
        ))
    return config
