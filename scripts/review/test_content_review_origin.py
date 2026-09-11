import io, sys, unittest
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parent))
import content_review_packet as review

class Response(io.BytesIO):
 def __enter__(self): return self
 def __exit__(self,*_): return False
class OriginTest(unittest.TestCase):
 def call(self, base, method):
  seen=[]
  def openit(request, timeout): seen.append(request); return Response(b'{}')
  with patch('urllib.request.urlopen', openit): review.Api(base,'cookie').request(method,'/x',{})
  return seen[0]
 def test_post_uses_https_origin_with_port_and_path_base(self): self.assertEqual('https://example.test:8443',self.call('https://example.test:8443/path','POST').get_header('Origin'))
 def test_reject_uses_http_origin(self): self.assertEqual('http://127.0.0.1:18081',self.call('http://127.0.0.1:18081','POST').get_header('Origin'))
 def test_get_has_no_origin(self): self.assertIsNone(self.call('http://127.0.0.1:18081','GET').get_header('Origin'))
 def test_bad_base_urls_refuse_before_network(self):
  for url in ('ftp://x','https://u:p@x','https://x/#f'):
   with self.assertRaises(review.Refused): review.Api(url,'cookie')
if __name__=='__main__': unittest.main()
