"""Builds the source trees of the stub distributions ``shinqlx-stubs`` and ``minqlx-stubs``.

``shinqlx-stubs`` carries the stubs of ``python/shinqlx`` unchanged. ``minqlx-stubs`` is generated from the
runtime shim ``python/minqlx``: every name the shim exports is re-exported from ``shinqlx``, so the shim stays the
single source of truth. Each distribution gets its own ``pyproject.toml`` and can be built with ``uv build``.
"""

from __future__ import annotations

import argparse
import ast
import re
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STUB_SOURCE = ROOT / "python" / "shinqlx"
SHIM_SOURCE = ROOT / "python" / "minqlx"
CARGO_MANIFEST = ROOT / "Cargo.toml"

HOMEPAGE = "https://github.com/mgaertne/shinqlx"
SHINQLX_README = f"""# shinqlx-stubs

Type stubs for [shinqlx]({HOMEPAGE}), a Quake Live server extension with a Python plugin interface. They are the stubs
of the shinqlx repository, which are checked against the extension with `stubtest`.

The extra `redis` installs the redis client that the stubs of the database refer to.
"""
MINQLX_README = f"""# minqlx-stubs

Type stubs for the module `minqlx` **as shinqlx provides it**, so for plugins that run on a [shinqlx]({HOMEPAGE})
server. They are not the stubs of the original [minqlx](https://github.com/MinoMino/minqlx).

A plugin that has to run on the original minqlx as well must stay within the names that both have. Compared with
minqlx 0.5.3 this package

* lacks `RedTeamChatChannel`, `BlueTeamChatChannel`, `FreeChatChannel` and `SpectatorChatChannel`. Use the constants
  such as `RED_TEAM_CHAT_CHANNEL` or `TeamChatChannel`, and the misspelt `PlayerDisonnectDispatcher` is spelled
  `PlayerDisconnectDispatcher`. `main_logger` and the modules that minqlx passes on, for example `minqlx.os`, are gone,
* has names that minqlx 0.5.3 does not have, among them the `damage` event with `DamageDispatcher`, the `DAMAGE_*`
  constants, `TeamChatChannel`, `get_targetting_entities`, `handle_damage` and `threading_excepthook`,
* differs in signatures: `slay_with_mod` takes `means_of_death` instead of `mod`, `thread` has no `force`, and
  `setex`, `lrem` and `zincrby` of the database take their values by keyword.

The page [Migration from minqlx]({HOMEPAGE}/blob/main/docs/migration.rst) lists all differences. This package needs
`shinqlx-stubs` of the same version.
"""
REDIS_REQUIREMENT = "redis>=5.0"
PRERELEASE_MARKERS = {"alpha": "a", "beta": "b", "rc": "rc"}
CARGO_VERSION = re.compile(r"(?P<release>\d+\.\d+\.\d+)(?:-(?P<kind>alpha|beta|rc)\.(?P<number>\d+))?")


def cargo_version(manifest: str) -> str:
    """Returns the version of the ``[workspace.package]`` table of a Cargo manifest."""
    section = re.search(r"^\[workspace\.package\]\n(?P<body>(?:(?!\[).*\n)*)", manifest, re.MULTILINE)
    match = section and re.search(r'^version\s*=\s*"(?P<version>[^"]+)"', section["body"], re.MULTILINE)
    if match is None:
        raise ValueError("no version in the [workspace.package] table")
    return match["version"]


def pep440_version(version: str) -> str:
    """Converts a Cargo version such as ``0.8.0-alpha.1`` to the PEP 440 form ``0.8.0a1``."""
    match = CARGO_VERSION.fullmatch(version)
    if match is None:
        raise ValueError(f"unsupported version: {version}")
    if match["kind"] is None:
        return match["release"]
    return f"{match['release']}{PRERELEASE_MARKERS[match['kind']]}{match['number']}"


def exported_names(tree: ast.Module) -> list[str]:
    """Returns the names of the ``__all__`` list of a module."""
    for node in tree.body:
        is_all = isinstance(node, ast.Assign) and any(
            isinstance(target, ast.Name) and target.id == "__all__" for target in node.targets
        )
        if is_all and isinstance(node.value, (ast.List, ast.Tuple)):
            return [element.value for element in node.value.elts if isinstance(element, ast.Constant)]
    raise ValueError("the module has no __all__ list")


def render_reexport_stub(source: str) -> str:
    """Renders a stub that re-exports each name of ``__all__`` from the module it is imported from."""
    tree = ast.parse(source)
    exported = exported_names(tree)
    imported = {
        alias.asname or alias.name: (node.module, alias.name)
        for node in tree.body
        if isinstance(node, ast.ImportFrom) and node.module and node.level == 0
        for alias in node.names
    }
    if missing := sorted(set(exported) - imported.keys()):
        raise ValueError(f"names in __all__ that are not imported: {', '.join(missing)}")

    by_module: dict[str, list[str]] = {}
    for name in sorted(exported):
        module, original = imported[name]
        by_module.setdefault(module, []).append(f"    {original} as {name},")

    lines: list[str] = []
    for module in sorted(by_module):
        lines += [f"from {module} import (", *by_module[module], ")"]
    lines += ["", "__all__ = [", *(f'    "{name}",' for name in sorted(exported)), "]", ""]
    return "\n".join(lines)


def render_pyproject(name: str, version: str, description: str, requirements: list[str], redis_extra: str) -> str:
    """Renders the ``pyproject.toml`` of a stub-only distribution that holds the package ``<name>``."""
    dependencies = ", ".join(f'"{requirement}"' for requirement in requirements)
    return f"""[build-system]
requires = ["setuptools>=61"]
build-backend = "setuptools.build_meta"

[project]
name = "{name}"
version = "{version}"
description = "{description}"
readme = "README.md"
requires-python = ">=3.8"
license = {{ text = "BSD-3-Clause" }}
classifiers = [
    "License :: OSI Approved :: BSD License",
    "Programming Language :: Python :: 3",
    "Programming Language :: Python :: 3 :: Only",
    "Typing :: Stubs Only",
]
dependencies = [{dependencies}]

[project.optional-dependencies]
redis = ["{redis_extra}"]

[project.urls]
Homepage = "{HOMEPAGE}"

[tool.setuptools]
packages = ["{name}"]

[tool.setuptools.package-data]
"{name}" = ["*.pyi", "py.typed"]
"""


def build_shinqlx_stubs(destination: Path, version: str) -> None:
    """Writes the distribution ``shinqlx-stubs`` with a copy of the stubs of the extension."""
    package = destination / "shinqlx-stubs"
    package.mkdir(parents=True)
    for stub in [*STUB_SOURCE.glob("*.pyi"), STUB_SOURCE / "py.typed"]:
        shutil.copyfile(stub, package / stub.name)
    pyproject = render_pyproject("shinqlx-stubs", version, "Type stubs for shinqlx", [], REDIS_REQUIREMENT)
    (destination / "pyproject.toml").write_text(pyproject)
    (destination / "README.md").write_text(SHINQLX_README)


def build_minqlx_stubs(destination: Path, version: str) -> None:
    """Writes the distribution ``minqlx-stubs`` with re-exports generated from the runtime shim."""
    package = destination / "minqlx-stubs"
    package.mkdir(parents=True)
    for shim in sorted(SHIM_SOURCE.glob("*.py")):
        (package / f"{shim.stem}.pyi").write_text(render_reexport_stub(shim.read_text()))
    pyproject = render_pyproject(
        "minqlx-stubs",
        version,
        "Type stubs for the minqlx module of shinqlx",
        [f"shinqlx-stubs=={version}"],
        f"shinqlx-stubs[redis]=={version}",
    )
    (destination / "pyproject.toml").write_text(pyproject)
    (destination / "README.md").write_text(MINQLX_README)


def build(output: Path, version: str) -> None:
    """Writes both distributions below ``output``, which must not exist or must be empty."""
    if output.exists() and any(output.iterdir()):
        raise FileExistsError(f"{output} is not empty")
    build_shinqlx_stubs(output / "shinqlx-stubs", version)
    build_minqlx_stubs(output / "minqlx-stubs", version)


def main() -> int:
    """Builds the distributions for the version of the Cargo workspace unless ``--version`` is given."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("output", type=Path, help="directory for the two distributions; new or empty")
    parser.add_argument("--version", help="PEP 440 version; default: the version of the Cargo workspace")
    arguments = parser.parse_args()
    try:
        version = arguments.version or pep440_version(cargo_version(CARGO_MANIFEST.read_text()))
        build(arguments.output, version)
    except (ValueError, FileExistsError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    print(f"stub distributions {version} written to {arguments.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
