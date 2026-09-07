"""Shared readers for Foundations authoring tests."""
import json


def find_kp(testdata,topic_id,kp_id):
    for topic in json.loads(testdata.read_text()):
        if topic["id"] != topic_id:
            continue
        for kp in topic["knowledge_points"]:
            if kp["id"] == kp_id:
                return kp
    raise KeyError((topic_id,kp_id))
