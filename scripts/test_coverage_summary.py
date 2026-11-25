#!/usr/bin/env python3
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "coverage_summary.py"


def write_report(path: Path, *, lines, functions, regions) -> None:
    payload = {
        "data": [
            {
                "totals": {
                    "lines": {
                        "covered": lines[0],
                        "count": lines[1],
                        "percent": lines[2],
                    },
                    "functions": {
                        "covered": functions[0],
                        "count": functions[1],
                        "percent": functions[2],
                    },
                    "regions": {
                        "covered": regions[0],
                        "count": regions[1],
                        "percent": regions[2],
                    },
                }
            }
        ]
    }
    path.write_text(json.dumps(payload), encoding="utf-8")


class CoverageSummaryTests(unittest.TestCase):
    def run_script(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), *args],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_renders_markdown_table_with_artifact_names(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            core = tmp_path / "core.json"
            cli = tmp_path / "cli.json"
            write_report(
                core,
                lines=(5544, 5544, 100.0),
                functions=(471, 471, 100.0),
                regions=(7943, 8029, 98.92888279985054),
            )
            write_report(
                cli,
                lines=(1768, 1768, 100.0),
                functions=(122, 122, 100.0),
                regions=(2342, 2396, 97.74624373956594),
            )

            result = self.run_script(
                "--report",
                f"devimg-core={core}",
                "--report",
                f"devimg-cli={cli}",
                "--artifact",
                "devimg-core=coverage-devimg-core-html",
                "--artifact",
                "devimg-cli=coverage-devimg-cli-html",
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("## Rust Coverage", result.stdout)
        self.assertIn(
            "| Report | Lines | Functions | Regions | HTML Artifact |", result.stdout
        )
        self.assertIn(
            "| devimg-core | 100.00% (5544/5544) | 100.00% (471/471) | 98.93% (7943/8029) | coverage-devimg-core-html |",
            result.stdout,
        )
        self.assertIn(
            "| devimg-cli | 100.00% (1768/1768) | 100.00% (122/122) | 97.75% (2342/2396) | coverage-devimg-cli-html |",
            result.stdout,
        )
        self.assertIn(
            "Line and function coverage are enforced at 100%; region coverage is informational.",
            result.stdout,
        )

    def test_writes_summary_to_output_path(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            report = tmp_path / "core.json"
            output = tmp_path / "summary.md"
            write_report(
                report,
                lines=(1, 2, 50.0),
                functions=(3, 4, 75.0),
                regions=(5, 8, 62.5),
            )

            result = self.run_script(
                "--report",
                f"devimg-core={report}",
                "--output",
                str(output),
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, "")
            self.assertIn("| devimg-core | 50.00% (1/2) |", output.read_text())

    def test_rejects_bad_report_argument(self) -> None:
        result = self.run_script("--report", "missing-separator")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("expected LABEL=PATH", result.stderr)

    def test_rejects_missing_report_file(self) -> None:
        result = self.run_script("--report", "core=/tmp/devimg-missing-coverage.json")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("coverage report not found", result.stderr)


if __name__ == "__main__":
    unittest.main()
