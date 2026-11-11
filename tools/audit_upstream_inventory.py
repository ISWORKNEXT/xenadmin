#!/usr/bin/env python3
"""Inventory pinned upstream implementation surfaces; no parity inference."""
import argparse
import csv
import json
from pathlib import Path
import re
import subprocess


def revision(path):
    return subprocess.check_output(['git', '-C', str(path), 'rev-parse', 'HEAD'], text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('upstream', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    upstream = args.upstream.resolve()
    repo = Path(__file__).resolve().parents[1]
    args.output.mkdir(parents=True, exist_ok=True)
    reviews = {p.name: p.read_text() for p in sorted((repo / 'docs').glob('audit-*-20261111.md'))}
    surfaces = {
        'commands': 'XenAdmin/Commands', 'actions': 'XenModel/Actions',
        'gui_actions': 'XenAdmin/Actions', 'wizards': 'XenAdmin/Wizards',
        'settings': 'XenAdmin/SettingsPanels', 'tabs': 'XenAdmin/TabPages',
        'diagnostics': 'XenAdmin/Diagnostics/Checks',
        'options': 'XenAdmin/Dialogs/OptionsPages', 'plugins': 'XenAdmin/Plugins',
    }
    counts = {}
    unmentioned = []
    with (args.output / 'upstream-surfaces.tsv').open('w', newline='') as stream:
        writer = csv.writer(stream, delimiter='\t', lineterminator='\n', quoting=csv.QUOTE_ALL)
        writer.writerow(['surface', 'upstream_path', 'declared_types', 'source_url', 'review_documents'])
        commit = revision(upstream)
        for surface, directory in surfaces.items():
            count = 0
            for path in sorted((upstream / directory).rglob('*.cs')):
                if path.name.lower().endswith('.designer.cs'):
                    continue
                relative = path.relative_to(upstream).as_posix()
                types = re.findall(r'\b(?:class|interface|enum|struct)\s+(\w+)', path.read_text(errors='replace'))
                references = [name for name, contents in reviews.items() if path.stem in contents]
                if not references:
                    unmentioned.append(relative)
                writer.writerow([surface, relative, ', '.join(types),
                                 f'https://github.com/xcp-ng/xenadmin/blob/{commit}/{relative}',
                                 '; '.join(references)])
                count += 1
            counts[surface] = count
    metadata = {'upstream_commit': revision(upstream), 'local_commit': revision(repo),
                'counts': counts, 'unmentioned_source_files': unmentioned,
                'note': 'File inventory only: helpers/base classes included. '
                'Review references are lexical filename-stem matches for navigation, not semantic parity proof. '
                'Counts are not feature counts. See manually reviewed domain tables.'}
    (args.output / 'inventory-summary.json').write_text(json.dumps(metadata, indent=2) + '\n')
    state = repo / 'native/src/state.rs'
    state_lines = state.read_text().splitlines()
    first = next(i for i, line in enumerate(state_lines) if line == 'pub enum UiAction {')
    last = next(i for i in range(first + 1, len(state_lines)) if state_lines[i] == '}')
    click_path = repo / 'docs/audit-live-ui-evidence-20261111/clicks/ui-action-coverage.json'
    clicks = json.loads(click_path.read_text()) if click_path.exists() else {}
    native_sources = {str(p.relative_to(repo)): p.read_text().splitlines()
                      for p in (repo / 'native/src').rglob('*.rs')}
    ui_rows = []
    for i in range(first + 1, last):
        match = re.match(r'^    ([A-Z]\w*)\s*[({,]', state_lines[i])
        if not match:
            continue
        name = match.group(1)
        sites = [f'{path}:{n}' for path, lines in native_sources.items()
                 for n, line in enumerate(lines, 1) if re.search(rf'\bUiAction::{name}\b', line)]
        clicked = clicks.get(name, {})
        ui_rows.append([name, f'native/src/state.rs:{i + 1}', '; '.join(sites),
                        clicked.get('status', 'NOT_VERIFIED_BY_CLICK'),
                        clicked.get('evidence', ''), clicked.get('note', '')])
    with (args.output / 'native-ui-actions.tsv').open('w', newline='') as stream:
        writer = csv.writer(stream, delimiter='\t', lineterminator='\n', quoting=csv.QUOTE_ALL)
        writer.writerow(['ui_action', 'declaration', 'source_sites', 'click_status', 'evidence', 'scope_note'])
        writer.writerows(ui_rows)
    ui_summary = {'ui_action_variants': len(ui_rows),
                  'click_verified_variants': sum(r[3] == 'CLICK_VERIFIED' for r in ui_rows),
                  'click_outcome_counts': {status: sum(r[3] == status for r in ui_rows)
                                           for status in sorted({r[3] for r in ui_rows})},
                  'note': 'Each enum variant may cover multiple objects, choices and states. '
                  'A verified click covers only the documented instance. Startup flags and backend calls do not count as clicks.'}
    (args.output / 'native-ui-summary.json').write_text(json.dumps(ui_summary, indent=2) + '\n')
    print(json.dumps(metadata, indent=2))
    print(json.dumps(ui_summary, indent=2))


if __name__ == '__main__':
    main()
