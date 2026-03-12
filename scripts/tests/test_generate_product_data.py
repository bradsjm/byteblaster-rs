import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SCRIPT_PATH = (
    Path(__file__).resolve().parents[1] / "generate_product_data.py"
)


def load_module():
    spec = importlib.util.spec_from_file_location("generate_product_data", SCRIPT_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class GenerateProductDataTests(unittest.TestCase):
    def setUp(self):
        self.module = load_module()

    def write_catalog(self, payload):
        tempdir = tempfile.TemporaryDirectory()
        path = Path(tempdir.name) / "catalog.json"
        path.write_text(json.dumps(payload), encoding="utf-8")
        self.addCleanup(tempdir.cleanup)
        return path

    def load_catalog(self, payload):
        path = self.write_catalog(payload)
        with mock.patch.object(self.module, "CATALOG_PATH", path):
            return self.module.load_catalog()

    def assert_load_error(self, payload, message_fragment):
        with self.assertRaises(SystemExit) as exc:
            self.load_catalog(payload)
        self.assertIn(message_fragment, str(exc.exception))

    def test_rejects_unknown_routing(self):
        self.assert_load_error(
            {
                "SVR": {
                    "wmo_prefix": "WU",
                    "title": "Severe Thunderstorm Warning",
                    "routing": "bogus",
                    "body_behavior": "catalog",
                    "extractors": ["vtec_events"],
                }
            },
            "unknown routing",
        )

    def test_rejects_unknown_body_behavior(self):
        self.assert_load_error(
            {
                "SVR": {
                    "wmo_prefix": "WU",
                    "title": "Severe Thunderstorm Warning",
                    "routing": "generic",
                    "body_behavior": "bogus",
                    "extractors": ["vtec_events"],
                }
            },
            "unknown body_behavior",
        )

    def test_rejects_catalog_body_behavior_with_empty_extractors(self):
        self.assert_load_error(
            {
                "SVR": {
                    "wmo_prefix": "WU",
                    "title": "Severe Thunderstorm Warning",
                    "routing": "generic",
                    "body_behavior": "catalog",
                    "extractors": [],
                }
            },
            "must define extractors",
        )

    def test_rejects_never_body_behavior_with_non_empty_extractors(self):
        self.assert_load_error(
            {
                "SIG": {
                    "wmo_prefix": "WS",
                    "title": "SIGMET bulletin",
                    "routing": "sigmet",
                    "body_behavior": "never",
                    "extractors": ["ugc"],
                }
            },
            "must not define extractors",
        )

    def test_requires_canonical_extractor_order(self):
        self.assert_load_error(
            {
                "SVR": {
                    "wmo_prefix": "WU",
                    "title": "Severe Thunderstorm Warning",
                    "routing": "generic",
                    "body_behavior": "catalog",
                    "extractors": ["ugc", "vtec_events"],
                }
            },
            "canonical order",
        )


if __name__ == "__main__":
    unittest.main()
