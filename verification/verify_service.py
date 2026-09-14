"""Private HTTP boundary. No model client, database, arbitrary code, or publish API."""
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
import time
from contracts import Unsupported, prepare_claim, prepare_problem
from lean_engine import Engine, digest

def unresolved(message):
    return {"status":"unresolved","message":message}

def verify_request(data, engine=None):
    engine = engine or Engine()
    source_hash = data.get("source_hash","") if isinstance(data,dict) else ""
    result = {"source_hash":source_hash,"status":"unresolved","learner":unresolved("Not verified."),
              "candidate":unresolved("Not verified."),"engine":engine.metadata(),"evidence":{},"supported":False}
    if not isinstance(data,dict) or set(data)!={"source_hash","problem","learner_answer","candidate_answer"}:
        result["learner"]=result["candidate"]=unresolved("Request fields do not match the schema.")
        return result
    if not isinstance(source_hash,str) or not 1<=len(source_hash)<=200 or not source_hash.isprintable():
        result["source_hash"]=""
        result["learner"]=result["candidate"]=unresolved("source_hash must be a printable string of 1..200 characters.")
        return result
    result["evidence"]["request_sha256"]=digest(json.dumps(data,sort_keys=True,separators=(",",":"),ensure_ascii=False))
    try:
        problem=prepare_problem(data["problem"])
    except (Unsupported,ValueError,RecursionError,OverflowError) as error:
        result["learner"]=result["candidate"]=unresolved(str(error))
        return result
    result["supported"]=True
    deadline=time.monotonic()+100
    for role,field in (("learner","learner_answer"),("candidate","candidate_answer")):
        try:
            claim=prepare_claim(problem,data[field])
        except (Unsupported,ValueError,TypeError,RecursionError,OverflowError) as error:
            result[role]=unresolved(str(error))
            continue
        try:
            accepted,message,evidence=engine.verify(claim,deadline)
        except (OSError,ValueError,MemoryError) as error:
            result[role]=unresolved("Verification infrastructure failed: "+type(error).__name__)
            continue
        result["evidence"][role]=evidence
        result[role]={"status":claim.status if accepted else "unresolved","message":message}
    if all(result[x]["status"] in ("proved","disproved") for x in ("learner","candidate")):
        result["status"]="completed"
    return result

class Handler(BaseHTTPRequestHandler):
    server_version="CadusVerifier/1"
    def setup(self):
        super().setup()
        self.connection.settimeout(5)
    def log_message(self,format,*args):
        pass
    def respond(self,status,body):
        data=json.dumps(body,ensure_ascii=True,separators=(",",":")).encode()
        self.send_response(status)
        self.send_header("Content-Type","application/json")
        self.send_header("Content-Length",str(len(data)))
        self.send_header("Cache-Control","no-store")
        self.end_headers()
        self.wfile.write(data)
    def do_GET(self):
        if self.path!="/health":
            self.respond(404,{"error":"not_found"})
            return
        engine=self.server.engine
        self.respond(200 if engine.available() else 503,{"status":"ready" if engine.available() else "unavailable","engine":engine.metadata()})
    def do_POST(self):
        if self.path!="/verify":
            self.respond(404,{"error":"not_found"})
            return
        if self.headers.get("Transfer-Encoding"):
            self.respond(400,{"error":"transfer_encoding_unsupported"})
            return
        if self.headers.get_content_type()!="application/json":
            self.respond(415,{"error":"application_json_required"})
            return
        try:
            length=int(self.headers.get("Content-Length","0"))
            if not 1<=length<=16384:
                self.respond(413,{"error":"body_size_out_of_bounds"})
                return
            raw=self.rfile.read(length)
            if len(raw)!=length:
                self.respond(400,{"error":"incomplete_body"})
                return
            def object_pairs(pairs):
                result={}
                for key,value in pairs:
                    if key in result:
                        raise ValueError("duplicate_json_key")
                    result[key]=value
                return result
            def constant(value):
                raise ValueError("nonfinite_json_number")
            data=json.loads(raw,object_pairs_hook=object_pairs,parse_constant=constant)
        except (ValueError,UnicodeDecodeError,RecursionError,TimeoutError):
            self.respond(400,{"error":"invalid_json"})
            return
        self.respond(200,verify_request(data,self.server.engine))

def main():
    server=HTTPServer((os.environ.get("VERIFIER_BIND","127.0.0.1"),int(os.environ.get("VERIFIER_PORT","8080"))),Handler)
    server.engine=Engine()
    server.serve_forever()

if __name__=="__main__":
    main()
