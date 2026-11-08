"""Runs mypy.stubtest against the Rust extension with runtime modules for the private stub modules.

The extension is a single module ``shinqlx``, but its stubs are split into private modules such as
``shinqlx._plugin``. These do not exist at runtime, so stubtest would skip them. Each of them gets a
module object holding the names that the stub declares, taken from ``shinqlx``.
"""

from __future__ import annotations

import ast
import importlib
import sys
import types
from pathlib import Path

from mypy import stubtest

PACKAGE = "shinqlx"
STUB_DIRECTORY = Path(__file__).resolve().parent.parent / "python" / PACKAGE


def declared_names(stub: Path) -> list[str]:
    """Returns the names that a stub file defines at its top level."""
    names: list[str] = []
    for node in ast.parse(stub.read_text()).body:
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            names.append(node.name)
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            names.append(node.target.id)
        elif isinstance(node, ast.Assign):
            names.extend(target.id for target in node.targets if isinstance(target, ast.Name))
    return names


def private_stub_modules() -> list[Path]:
    """Returns the stubs of the private modules of the package."""
    return sorted(path for path in STUB_DIRECTORY.glob("_*.pyi") if path.stem != "__init__")


def runtime_module(stub: Path, runtime: types.ModuleType) -> types.ModuleType:
    """Creates the module for a stub with those declared names that exist in the extension."""
    module = types.ModuleType(f"{PACKAGE}.{stub.stem}")
    for name in declared_names(stub):
        if hasattr(runtime, name):
            setattr(module, name, getattr(runtime, name))
    return module


def main() -> int:
    """Provides the runtime modules and runs stubtest with the arguments of the command line."""
    runtime = importlib.import_module(PACKAGE)
    for stub in private_stub_modules():
        module = runtime_module(stub, runtime)
        sys.modules[module.__name__] = module
    return stubtest.main()


if __name__ == "__main__":
    sys.exit(main())
