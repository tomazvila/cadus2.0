"""The zero-cost adapter resolves explicit drafts and refuses unknown keys."""
import json
import threading
import unittest
import urllib.error
import urllib.request
from draft_endpoint import server_for


class DraftEndpointTest(unittest.TestCase):
    def test_literal_draft_and_unknown_key(self):
        draft = {"hints": ["Count on from the first addend."]}
        server = server_for(0, {("single-digit-addition/kp1", "hint_ladder"): draft})
        thread = threading.Thread(target=server.serve_forever)
        thread.start()
        try:
            request = {
                "messages": [{"role": "user", "content": "Topic: Addition (id: single-digit-addition)\nKnowledge point: Sums (id: kp1)"}],
                "tool_choice": {"function": {"name": "emit_hint_ladder"}},
            }
            url = f"http://127.0.0.1:{server.server_address[1]}/v1/chat/completions"
            def post():
                return urllib.request.urlopen(urllib.request.Request(url, json.dumps(request).encode(), {"Content-Type": "application/json"}), timeout=2)
            with post() as response:
                body = json.load(response)
            self.assertEqual(body["usage"]["cost"], 0)
            self.assertEqual(json.loads(body["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]), draft)
            request["messages"][0]["content"] = request["messages"][0]["content"].replace("kp1", "kp9")
            with self.assertRaises(urllib.error.HTTPError) as error:
                post()
            self.assertEqual(error.exception.code, 400)
        finally:
            server.shutdown()
            server.server_close()
            thread.join()


if __name__ == "__main__":
    unittest.main()
