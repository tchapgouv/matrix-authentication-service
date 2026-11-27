#!/usr/bin/env python3
"""Check that the Tchap translations are complete with respect to upstream.

How translations work in this fork (see Dockerfile):

* the runtime translation directory is fully replaced by
  ``tchap/resources/translations/`` — upstream ships more languages, Tchap
  intentionally keeps only ``en`` and ``fr``;
* the runtime templates are the upstream ``templates/`` directory with the
  ``tchap/resources/templates/`` overlay applied on top (a file with the same
  relative path overrides the upstream one).

Therefore the Tchap translation files must contain every key of the
corresponding upstream files, and every key used by the effective template
set. A missing key fails at render time with::

    could not render template "pages/xxx.html"
    invalid operation: Missing translation

Checks performed
----------------
1. For each language present in BOTH directories: every key of
   ``translations/<lang>.json`` must exist in
   ``tchap/resources/translations/<lang>.json`` (``en`` is the fallback
   locale, so a missing ``en`` key breaks rendering in every language).
2. Every translation key used by the effective template set (upstream
   templates + Tchap overlay) must exist in the Tchap ``en.json``. This
   catches keys used by Tchap-specific templates, which the upstream i18n
   extractor doesn't know about.

``fr`` is intentionally partial (like upstream's own ``fr.json``): keys
missing from ``fr.json`` fall back to ``en`` at runtime.

Usage::

    python3 tchap/check_translations.py [--diff-values]

Exit codes: 0 = complete, 1 = incomplete, 2 = tooling error.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

# Matches translation calls: _("key"), _p("key"), _n('key').
# Icon macros like `icon.error_solid(` don't match: there is no word
# boundary right before the `_` inside `error_solid`.
USAGE_RE = re.compile(r"\b_[pn]?\(\s*(['\"])(.+?)\1")
TEMPLATE_SUFFIXES = {".html", ".txt", ".subject"}


def flatten(obj: object, prefix: str = "") -> dict[str, object]:
    """Flatten nested dicts into a ``{dotted.path: value}`` mapping."""
    out: dict[str, object] = {}
    assert isinstance(obj, dict)
    for key, value in obj.items():
        assert isinstance(key, str)
        path = f"{prefix}{key}"
        if isinstance(value, dict):
            out.update(flatten(value, path + "."))
        else:
            out[path] = value
    return out


def load(path: Path) -> dict:
    try:
        with path.open(encoding="utf-8") as f:
            return json.load(f)
    except OSError as e:
        die(f"cannot read {path}: {e}")
    except json.JSONDecodeError as e:
        die(f"invalid JSON in {path}: {e}")
    raise AssertionError("unreachable")


def die(message: str) -> None:
    print(f"[FATAL] {message}", file=sys.stderr)
    sys.exit(2)


def is_metadata_key(key: str) -> bool:
    """i18n extractor metadata keys, e.g. ``action.@back.context``."""
    return any(segment.startswith("@") for segment in key.split("."))


def effective_templates(upstream: Path, overlay: Path) -> dict[str, Path]:
    """Runtime template set: upstream templates + Tchap overlay on top."""
    files: dict[str, Path] = {}
    for base in (upstream, overlay):
        if not base.is_dir():
            die(f"template directory not found: {base}")
        for path in sorted(base.rglob("*")):
            if path.is_file() and path.suffix in TEMPLATE_SUFFIXES:
                files[path.relative_to(base).as_posix()] = path
    return files


def extract_usages(files: dict[str, Path]) -> dict[str, list[str]]:
    """Map each translation key used in templates to its usage locations."""
    usages: dict[str, list[str]] = {}
    for rel, path in files.items():
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except OSError as e:
            die(f"cannot read {path}: {e}")
        for lineno, line in enumerate(lines, 1):
            for match in USAGE_RE.finditer(line):
                usages.setdefault(match.group(2), []).append(f"{rel}:{lineno}")
    return usages


def check_completeness(lang: str, upstream: dict, tchap: dict) -> int:
    """Report keys that upstream has but the Tchap file lacks."""
    up = flatten(upstream)
    tc = flatten(tchap)
    missing = sorted(set(up) - set(tc))
    extra = sorted(set(tc) - set(up))

    label = f"{lang}.json"
    if missing:
        missing_str = [k for k in missing if not is_metadata_key(k)]
        missing_meta = [k for k in missing if is_metadata_key(k)]
        print(
            f"[ERROR] {label}: {len(missing)} upstream key(s) missing from "
            f"tchap/resources/translations/{lang}.json "
            f"(rendering will fail with 'Missing translation'):"
        )
        for key in missing_str:
            print(f"    - {key}")
        if missing_meta:
            print(
                f"    ({len(missing_meta)} i18n metadata key(s) also missing: "
                + ", ".join(missing_meta)
                + ")"
            )
        return len(missing)

    print(f"[OK] {label}: all {len(up)} upstream keys present")
    if extra:
        print(
            f"[info] {label}: {len(extra)} Tchap-only key(s) "
            "(expected for Tchap-specific features)"
        )
    return 0


def show_value_diffs(lang: str, upstream: dict, tchap: dict) -> None:
    """Opt-in: list keys whose value differs between upstream and Tchap."""
    up = flatten(upstream)
    tc = flatten(tchap)
    changed = sorted(k for k in set(up) & set(tc) if up[k] != tc[k])
    real = [k for k in changed if not is_metadata_key(k)]
    meta = [k for k in changed if is_metadata_key(k)]
    if real:
        print(f"[diff] {lang}.json: {len(real)} key(s) with a different value:")
        for key in real:
            print(f"    - {key}")
            print(f"        upstream: {up[key]!r}")
            print(f"        tchap   : {tc[key]!r}")
    if meta:
        print(
            f"[diff] {lang}.json: {len(meta)} metadata key(s) with different "
            "template positions (regenerated by the i18n extractor, ignorable)"
        )


def check_template_usages(
    usages: dict[str, list[str]], tchap_en: dict, tchap_dir: Path
) -> int:
    """Report keys used by templates but missing from the Tchap en.json."""
    en = flatten(tchap_en)
    missing = {k: locs for k, locs in sorted(usages.items()) if k not in en}
    if missing:
        print(
            "[ERROR] key(s) used by templates but missing from "
            f"{tchap_dir / 'en.json'}:"
        )
        for key, locs in missing.items():
            shown = ", ".join(locs[:3]) + (", …" if len(locs) > 3 else "")
            print(f"    - {key}  (used in {shown})")
        return len(missing)

    print(f"[OK] all {len(usages)} key(s) used by templates present in en.json")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Check Tchap translations completeness vs upstream"
    )
    parser.add_argument(
        "--mas-home",
        type=Path,
        default=Path(__file__).resolve().parent.parent,
        help="MAS repository root (default: parent of this script's directory)",
    )
    parser.add_argument(
        "--diff-values",
        action="store_true",
        help="also list keys whose value differs from upstream "
        "(informational: Tchap intentionally rewrites some strings)",
    )
    args = parser.parse_args(argv)

    root: Path = args.mas_home
    upstream_tr = root / "translations"
    tchap_tr = root / "tchap" / "resources" / "translations"
    upstream_tpl = root / "templates"
    tchap_tpl = root / "tchap" / "resources" / "templates"

    if not upstream_tr.is_dir() or not tchap_tr.is_dir():
        die(f"translations directories not found under {root}")

    errors = 0

    # Check 1: every upstream key must exist in the Tchap files
    upstream_langs = {p.stem for p in upstream_tr.glob("*.json")}
    tchap_langs = {p.stem for p in tchap_tr.glob("*.json")}
    for lang in sorted(upstream_langs & tchap_langs):
        errors += check_completeness(
            lang,
            load(upstream_tr / f"{lang}.json"),
            load(tchap_tr / f"{lang}.json"),
        )
    skipped = sorted(upstream_langs - tchap_langs)
    if skipped:
        print(
            f"[info] language(s) intentionally not shipped by Tchap: "
            f"{', '.join(skipped)}"
        )

    # Check 2: every key used by the effective templates must be in en.json
    files = effective_templates(upstream_tpl, tchap_tpl)
    usages = extract_usages(files)
    overlay_count = sum(
        1
        for p in tchap_tpl.rglob("*")
        if p.is_file() and p.suffix in TEMPLATE_SUFFIXES
    )
    print(
        f"[info] scanned {len(files)} effective template file(s) "
        f"({overlay_count} file(s) in the Tchap overlay)"
    )
    errors += check_template_usages(usages, load(tchap_tr / "en.json"), tchap_tr)

    # Opt-in value diff
    if args.diff_values:
        for lang in sorted(upstream_langs & tchap_langs):
            show_value_diffs(
                lang,
                load(upstream_tr / f"{lang}.json"),
                load(tchap_tr / f"{lang}.json"),
            )

    if errors:
        print(f"\nFAIL: {errors} missing key(s) — sync tchap/resources/translations/")
        return 1
    print("\nOK: Tchap translations are complete with respect to upstream")
    return 0


if __name__ == "__main__":
    sys.exit(main())
