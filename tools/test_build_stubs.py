"""Tests of ``build_stubs``: the generated stubs must follow the shim, the stubs and the Cargo version."""

from __future__ import annotations

import ast
import tempfile
import unittest
from pathlib import Path

import build_stubs

VERSION = "0.8.0a1"
OLDEST_PYTHON = (3, 8)


def literal_all(path: Path) -> set[str]:
    """Reads the ``__all__`` list of a module without importing it."""
    for node in ast.parse(path.read_text()).body:
        if isinstance(node, ast.Assign) and [target.id for target in node.targets] == ["__all__"]:
            return set(ast.literal_eval(node.value))
    raise AssertionError(f"{path} has no __all__")


class VersionTests(unittest.TestCase):
    def test_cargo_prerelease_becomes_pep440(self) -> None:
        cases = {
            "0.8.0-alpha.1": "0.8.0a1",
            "1.0.0-beta.3": "1.0.0b3",
            "0.9.0-rc.2": "0.9.0rc2",
            "1.2.3": "1.2.3",
        }
        for cargo, expected in cases.items():
            with self.subTest(cargo):
                self.assertEqual(expected, build_stubs.pep440_version(cargo))

    def test_unsupported_versions_are_rejected(self) -> None:
        for version in ("1.2", "1.2.3-dev.1", "1.2.3+build"):
            with self.subTest(version), self.assertRaises(ValueError):
                build_stubs.pep440_version(version)

    def test_version_is_read_from_the_workspace_package_table(self) -> None:
        manifest = (
            "[workspace]\nmembers = []\n\n"
            '[workspace.package]\nedition = "2024"\nversion = "0.8.0-alpha.1"\n\n'
            '[package]\nversion = "9.9.9"\n'
        )
        self.assertEqual("0.8.0-alpha.1", build_stubs.cargo_version(manifest))

    def test_manifest_without_workspace_version_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            build_stubs.cargo_version('[package]\nversion = "1.0.0"\n')

    def test_version_of_this_repository_converts(self) -> None:
        version = build_stubs.cargo_version(build_stubs.CARGO_MANIFEST.read_text())
        self.assertRegex(build_stubs.pep440_version(version), r"^\d+\.\d+\.\d+((a|b|rc)\d+)?$")


class ReexportTests(unittest.TestCase):
    def test_names_are_reexported_from_their_modules(self) -> None:
        source = (
            "import shinqlx\n"
            "from shinqlx import Zeta, Alpha\n"
            "from shinqlx.database import Redis as Store\n"
            '__all__ = ["Zeta", "Alpha", "Store"]\n'
        )
        expected = (
            "from shinqlx import (\n"
            "    Alpha as Alpha,\n"
            "    Zeta as Zeta,\n"
            ")\n"
            "from shinqlx.database import (\n"
            "    Redis as Store,\n"
            ")\n"
            "\n"
            "__all__ = [\n"
            '    "Alpha",\n'
            '    "Store",\n'
            '    "Zeta",\n'
            "]\n"
        )
        self.assertEqual(expected, build_stubs.render_reexport_stub(source))

    def test_type_names_are_added_to_the_exports(self) -> None:
        stub = build_stubs.type_only_names(
            "from typing import TypedDict\nfrom typing_extensions import TypeAlias\n"
            "class Data(TypedDict): ...\nclass _Hidden(TypedDict): ...\n"
            "Result: TypeAlias = int | None\nclass Plain: ...\nclass Unlisted(TypedDict): ...\n"
            '__all__ = ["Data", "Result", "Plain"]\n'
        )
        self.assertEqual(["Data", "Result"], stub)
        rendered = build_stubs.render_reexport_stub("from shinqlx import Alpha\n__all__ = ['Alpha']\n", stub)
        self.assertIn("    Data as Data,\n", rendered)
        self.assertIn('    "Result",\n', rendered)

    def test_name_that_is_not_imported_is_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "Local"):
            build_stubs.render_reexport_stub('from shinqlx import Alpha\n__all__ = ["Alpha", "Local"]\n')

    def test_module_without_all_is_rejected(self) -> None:
        with self.assertRaises(ValueError):
            build_stubs.render_reexport_stub("from shinqlx import Alpha\n")


class BuildTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.output = Path(self.directory.name) / "stubs"
        build_stubs.build(self.output, VERSION)

    def test_shinqlx_stubs_hold_every_stub_and_the_marker(self) -> None:
        expected = {path.name for path in build_stubs.STUB_SOURCE.glob("*.pyi")} | {"py.typed"}
        package = self.output / "shinqlx-stubs" / "shinqlx-stubs"
        self.assertEqual(expected, {path.name for path in package.iterdir()})

    def test_minqlx_stubs_export_what_the_shim_exports_and_the_type_names(self) -> None:
        package = self.output / "minqlx-stubs" / "minqlx-stubs"
        type_names = set(build_stubs.type_only_names((build_stubs.STUB_SOURCE / "__init__.pyi").read_text()))
        for shim in build_stubs.SHIM_SOURCE.glob("*.py"):
            expected = literal_all(shim) | (type_names if shim.stem == "__init__" else set())
            with self.subTest(shim.name):
                self.assertEqual(expected, literal_all(package / f"{shim.stem}.pyi"))

    def test_type_names_are_the_typed_dicts_and_aliases_of_the_stubs(self) -> None:
        names = set(build_stubs.type_only_names((build_stubs.STUB_SOURCE / "__init__.pyi").read_text()))
        self.assertLessEqual(
            {"CancellableEventReturn", "UncancellableEventReturn", "GameStartData", "KillData", "StatsData"}, names
        )
        self.assertTrue(all(not name.startswith("_") for name in names))

    def test_minqlx_stubs_export_only_names_that_shinqlx_stubs_export(self) -> None:
        shinqlx_names = literal_all(build_stubs.STUB_SOURCE / "__init__.pyi")
        minqlx_names = literal_all(self.output / "minqlx-stubs" / "minqlx-stubs" / "__init__.pyi")
        self.assertEqual(set(), minqlx_names - shinqlx_names)

    def test_all_stubs_are_valid_for_the_oldest_python(self) -> None:
        for stub in self.output.rglob("*.pyi"):
            with self.subTest(stub.name):
                ast.parse(stub.read_text(), feature_version=OLDEST_PYTHON)

    def test_distributions_are_stub_only_and_minqlx_pins_shinqlx(self) -> None:
        shinqlx = (self.output / "shinqlx-stubs" / "pyproject.toml").read_text()
        minqlx = (self.output / "minqlx-stubs" / "pyproject.toml").read_text()
        for name, text in (("shinqlx-stubs", shinqlx), ("minqlx-stubs", minqlx)):
            with self.subTest(name):
                self.assertIn(f'name = "{name}"', text)
                self.assertIn(f'version = "{VERSION}"', text)
                self.assertIn('requires-python = ">=3.8"', text)
                self.assertIn("Typing :: Stubs Only", text)
        self.assertIn(f'dependencies = ["shinqlx-stubs=={VERSION}"]', minqlx)
        self.assertIn("dependencies = []", shinqlx)

    def test_readme_tells_users_of_the_original_about_the_differences(self) -> None:
        minqlx = (self.output / "minqlx-stubs" / "README.md").read_text()
        self.assertIn("as shinqlx provides it", minqlx)
        self.assertIn("0.5.3", minqlx)
        self.assertIn("PlayerDisonnectDispatcher", minqlx)
        for name in ("shinqlx-stubs", "minqlx-stubs"):
            with self.subTest(name):
                self.assertIn('readme = "README.md"', (self.output / name / "pyproject.toml").read_text())
                self.assertTrue((self.output / name / "README.md").is_file())

    def test_existing_output_is_not_overwritten(self) -> None:
        with self.assertRaises(FileExistsError):
            build_stubs.build(self.output, VERSION)


if __name__ == "__main__":
    unittest.main()
