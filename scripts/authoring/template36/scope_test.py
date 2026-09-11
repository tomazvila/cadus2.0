"""Manifest validity and isolation against the exact legacy blocker inventory."""
import json
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'scripts/authoring'))
sys.path.insert(0, str(ROOT / 'scripts/review'))
from import_local_drafts import load_document, validate
import foundations_content_audit as audit


class TemplateScopeTests(unittest.TestCase):
    def test_manifest_is_importer_valid_and_exactly_replaces_legacy_keys(self):
        path = ROOT / 'docs/content-foundations/symbolic-reviewed-recipes.json'
        document, rows = load_document(path)
        drafts = validate(document, rows)
        keys = set(json.loads((path.parent / 'template36/keys.json').read_text()))
        self.assertEqual({key for key, kind in drafts}, keys)
        self.assertEqual(len(drafts), 36)
        self.assertTrue(all(kind == 'template' for key, kind in drafts))
        self.assertEqual(document['status'], 'pending')
        pending = audit.pending_templates(path.parent)
        for key in keys:
            self.assertEqual(len(pending[key]), 1, key)
            self.assertEqual(set(pending[key][0]['document']), {'kp_id','kind','arguments'})

    def test_live_content_audit_is_clean_for_slice_and_unchanged_elsewhere(self):
        facts = json.loads((ROOT / 'target/template36-facts.json').read_text())
        keys = set(json.loads((ROOT / 'docs/content-foundations/template36/keys.json').read_text()))
        pending = audit.pending_templates(ROOT / 'docs/content-foundations')
        current = audit.build_report(facts, pending)
        source = subprocess.check_output(['git', 'show',
            'a3c6c5f598f45c5bfbd271915f80e101b45e6dd8:docs/content-foundations/symbolic-reviewed-recipes.json'],
            cwd=ROOT, text=True)
        old = json.loads(source)['templates']
        self.assertEqual({row['kp_id'] for row in old}, keys)
        baseline = {key: value for key, value in pending.items() if key not in keys}
        baseline.update({row['kp_id']: [{'document':row, 'source':'legacy'}] for row in old})
        before = audit.build_report(facts, baseline)
        self.assertEqual([r for r in current['kps'] if r['kp_key'] not in keys],
                         [r for r in before['kps'] if r['kp_key'] not in keys])
        owned = [r for r in current['kps'] if r['kp_key'] in keys]
        self.assertEqual(len(owned), 36)
        self.assertTrue(all(not r['issues'] for r in owned), owned)


if __name__ == '__main__':
    unittest.main()
