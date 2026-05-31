#!/usr/bin/env python3
import argparse
import json
import sys
from pathlib import Path


def parse_mapping(raw: str, *, kind: str) -> tuple[str, str]:
    if "=" not in raw:
        raise argparse.ArgumentTypeError(f"{kind}: expected LABEL=PATH")
    label, value = raw.split("=", 1)
    label = label.strip()
    value = value.strip()
    if not label or not value:
        raise argparse.ArgumentTypeError(f"{kind}: expected LABEL=PATH")
    return label, value


def load_totals(path: Path) -> dict:
    if not path.exists():
        raise FileNotFoundError(f"coverage report not found: {path}")
    with path.open(encoding="utf-8") as handle:
        payload = json.load(handle)
    try:
        return payload["data"][0]["totals"]
    except (KeyError, IndexError, TypeError) as error:
        raise ValueError(f"invalid coverage report shape: {path}") from error


def metric_cell(metric: dict) -> str:
    covered = metric["covered"]
    count = metric["count"]
    percent = metric["percent"]
    return f"{percent:.2f}% ({covered}/{count})"


def render_summary(reports: list[tuple[str, Path]], artifacts: dict[str, str]) -> str:
    lines = [
        "## Rust Coverage",
        "",
        "| Report | Lines | Functions | Regions | HTML Artifact |",
        "| --- | ---: | ---: | ---: | --- |",
    ]

    for label, path in reports:
        totals = load_totals(path)
        artifact = artifacts.get(label, "-")
        lines.append(
            "| {label} | {lines} | {functions} | {regions} | {artifact} |".format(
                label=label,
                lines=metric_cell(totals["lines"]),
                functions=metric_cell(totals["functions"]),
                regions=metric_cell(totals["regions"]),
                artifact=artifact,
            )
        )

    lines.extend(
        [
            "",
            "Line and function coverage are enforced at 100%; region coverage is informational.",
            "Download the HTML artifacts from this workflow run for source-level highlighting.",
            "",
        ]
    )
    return "\n".join(lines)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Render a Markdown summary from cargo-llvm-cov JSON reports."
    )
    parser.add_argument(
        "--report",
        action="append",
        required=True,
        metavar="LABEL=PATH",
        help="Coverage JSON report to include.",
    )
    parser.add_argument(
        "--artifact",
        action="append",
        default=[],
        metavar="LABEL=NAME",
        help="HTML artifact name to display for a report label.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Write Markdown to this path instead of stdout.",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    try:
        reports = [
            (label, Path(path))
            for label, path in (
                parse_mapping(raw, kind="report") for raw in args.report
            )
        ]
        artifacts = dict(
            parse_mapping(raw, kind="artifact") for raw in args.artifact
        )
        summary = render_summary(reports, artifacts)
    except (argparse.ArgumentTypeError, FileNotFoundError, ValueError, KeyError) as error:
        print(error, file=sys.stderr)
        return 2

    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(summary, encoding="utf-8")
    else:
        sys.stdout.write(summary)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
