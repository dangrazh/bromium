"""Offline signature/export checks against the installed development extension."""
import ast
import inspect
import re
from pathlib import Path
import sys
import unittest
import bromium

class ApiContractTests(unittest.TestCase):
    def test_stub_exports_signatures_and_defaults(self):
        tree = ast.parse(Path(__file__).parents[1].joinpath("bromium.pyi").read_text(encoding="utf-8"))
        checks = [(bromium, item) for item in tree.body if isinstance(item, ast.FunctionDef)]
        for cls in (n for n in tree.body if isinstance(n, ast.ClassDef)):
            self.assertTrue(hasattr(bromium, cls.name), cls.name)
            owner = getattr(bromium, cls.name)
            for method in (n for n in cls.body if isinstance(n, ast.FunctionDef)):
                if method.name.startswith("__") and method.name != "__init__":
                    continue  # CPython slot wrappers have generic positional signatures.
                decorators = [ast.unparse(d) for d in method.decorator_list]
                if any(d == "property" or d.endswith(".setter") for d in decorators):
                    self.assertTrue(hasattr(owner, method.name), f"{cls.name}.{method.name}")
                else:
                    checks.append((owner, method))
        for owner, node in checks:
            with self.subTest(owner=owner.__name__, method=node.name):
                target = owner if node.name == "__init__" else getattr(owner, node.name)
                actual = [p for p in inspect.signature(target).parameters.values() if p.name not in ("self", "cls")]
                args = node.args.posonlyargs + node.args.args
                defaults = [inspect.Parameter.empty] * (len(args)-len(node.args.defaults)) + [ast.literal_eval(d) for d in node.args.defaults]
                expected = [(a.arg, d) for a,d in zip(args, defaults) if a.arg not in ("self", "cls")]
                self.assertEqual([(p.name,p.default) for p in actual], expected)

    def test_snapshot_value_types_and_exception_hierarchy(self):
        element = bromium.Element("name", "//Button", 0, "Button", [42,1], (1,2,3,4))
        self.assertIsInstance(element.name, str)
        self.assertEqual(element.runtime_id, [42,1])
        self.assertEqual(element.bounding_rectangle, (1,2,3,4))
        self.assertIsInstance(bromium.get_version(), str)
        self.assertTrue(issubclass(bromium.StaleTreeError, TimeoutError))
        self.assertTrue(issubclass(bromium.TreeConstructionError, TimeoutError))

    def test_close_rejects_empty_identity(self):
        element = bromium.Element("", "", 0, "Window", [], (0, 0, 0, 0))
        with self.assertRaisesRegex(bromium.ElementNotFoundError, "Empty runtime ID"):
            element.close()  # rejects before any COM/provider access

    def test_readme_python_examples_compile(self):
        root = Path(__file__).resolve().parents[3]
        for path in (root / "README.md", root / "crates/bromium/README.md"):
            examples = re.findall(r"```python\s*\n(.*?)```", path.read_text(encoding="utf-8"), re.DOTALL)
            self.assertTrue(examples, str(path))
            for index, source in enumerate(examples):
                with self.subTest(readme=str(path), example=index):
                    compile(source, str(path), "exec")  # do not execute desktop actions

    def test_stub_covers_runtime_public_exports(self):
        tree = ast.parse(Path(__file__).parents[1].joinpath("bromium.pyi").read_text(encoding="utf-8"))
        declared = {node.name for node in tree.body if isinstance(node, (ast.ClassDef, ast.FunctionDef))}
        exported = {name for name, value in vars(bromium).items()
                    if not name.startswith("_") and (inspect.isclass(value) or inspect.isbuiltin(value))}
        self.assertEqual(exported, declared)

    def test_screen_metadata_and_logging_string_contract(self):
        screen = bromium.ScreenInfo(1, "test", "fixture", -10, 20, 100, 200, 30, 40, 0.0, 1.25, 60.0, True)
        for name in ("id", "x", "y", "width", "height", "width_mm", "height_mm"):
            self.assertIsInstance(getattr(screen, name), int)
        for name in ("rotation", "scale_factor", "frequency"):
            self.assertIsInstance(getattr(screen, name), float)
        self.assertIsInstance(screen.is_primary, bool)
        previous = bromium.get_log_level()
        try:
            bromium.set_log_level("warning")
            self.assertEqual(bromium.get_log_level().lower(), "warn")
            bromium.set_log_level("not a level")
            self.assertEqual(bromium.get_log_level().lower(), "info")
            with self.assertRaises(TypeError):
                bromium.set_log_level(bromium.LogLevel.Info)
        finally:
            bromium.set_log_level(previous)

if __name__ == "__main__":
    unittest.main(testRunner=unittest.TextTestRunner(stream=sys.stdout, verbosity=2))
