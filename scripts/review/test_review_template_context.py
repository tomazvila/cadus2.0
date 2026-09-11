"""AI review binds the authoritative eligible template bank and its lifetime."""
import json
import tempfile
import unittest
from pathlib import Path
import ai_content_review as ai
import content_review_packet as packet
from test_content_review_packet import FakeApi, document


class ContextApi(FakeApi):
    def list_status(self, status):
        return [row for row in super().list_pending() if row['status'] == status]

    def decide(self, digest, decision, reason=None, policy_digest=None, template_context_digest=None):
        if decision == 'approve' and template_context_digest != self.documents[digest]['template_context_digest']:
            raise packet.Refused('review_context_changed')
        return super().decide(digest, decision, reason, policy_digest, template_context_digest)


class TemplateContextTest(unittest.TestCase):
    def test_context_and_membership_are_fingerprint_inputs(self):
        row = document('teach')
        baseline = packet.fingerprint(row)
        for field, value in [('template_context_digest','new'), ('approved_template_context_digest','old'),
                             ('eligible_template_digests',['t1'])]:
            self.assertNotEqual(baseline,packet.fingerprint(row | {field:value}))
        with self.assertRaises(packet.Refused):
            packet.fingerprint({key:value for key,value in row.items() if key!='eligible_template_digests'})

    def test_same_policy_stale_bank_approval_is_exported_and_reapproved(self):
        row = document('teach') | {'status':'approved','template_context_digest':'current-bank',
                                  'approved_template_context_digest':'old-bank','eligible_template_digests':['t1','t2']}
        api=ContextApi([row])
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'packet.json'
            count,decisions,_=packet.export_packet(api,path,None,1,stale_policy=True)
            self.assertEqual(count,1)
            exported=packet.load_json(path)
            decisions.write_text(json.dumps({'decision_version':1,'packet_sha256':exported['packet_sha256'],
                                             'decisions':[{'digest':'teach','decision':'approve'}]}))
            self.assertTrue(packet.apply_decisions(api,path,decisions,True)['complete'])
            self.assertEqual(api.documents['teach']['approved_template_context_digest'],'current-bank')
            self.assertFalse(packet.reviewable(api.document('teach')))

    def test_bank_race_at_write_cannot_approve_unreviewed_context(self):
        api=ContextApi([document('teach') | {'template_context_digest':'reviewed'}])
        item=packet.packet_item(api.document('teach'),{})
        original=api.decide
        def raced(*args):
            api.documents['teach']['template_context_digest']='changed'
            return original(*args)
        api.decide=raced
        with self.assertRaisesRegex(packet.Refused,'review_context_changed'):
            packet.apply_one_decision(api,item,'approve','reviewed',{},None,None)
        self.assertEqual(api.writes,[])

    def test_finite_context_uses_only_authoritative_members(self):
        old=document('old',kind='template') | {'status':'approved','policy_digest':'finite','approved_policy_digest':'finite'}
        new=old | {'digest':'new'}
        teach=document('teach') | {'policy_digest':'finite','eligible_template_digests':['new']}
        api=ContextApi([old,new,teach])
        self.assertEqual([row['digest'] for row in ai.current_templates(api,teach)],['new'])
        api.documents['new']['status']='rejected'
        with self.assertRaisesRegex(packet.Refused,'bank changed'):
            ai.current_templates(api,teach)

    def test_finite_candidate_prospective_bank_replaces_previous_template(self):
        old=document('old',kind='template') | {'status':'approved','policy_digest':'finite','approved_policy_digest':'finite'}
        new=document('new',kind='template') | {'policy_digest':'finite','eligible_template_digests':['old']}
        item=packet.packet_item(new,{})
        self.assertEqual(ai.serving_hash(ContextApi([old,new]),item),
                         packet.sha256([{'digest':'new','fingerprint_sha256':item['fingerprint_sha256']}]))

    def test_api_posts_both_reviewed_contexts(self):
        api=packet.Api('http://localhost','test-cookie');calls=[]
        api.request=lambda *args:calls.append(args)
        api.decide('teach','approve',policy_digest=None,template_context_digest='bank')
        self.assertEqual(calls,[('POST','/teach/approve',{'policy_digest':None,'template_context_digest':'bank'})])
