"""Run inside the isolated verifier container with the HTTP service already running."""
import hashlib
import json
import sys
import time
import urllib.error
import urllib.request

CASES = [
    ("factor24_pairs", {"kind":"factor_list","target":24},
     "1\u00d724,2X12,3*8,4 times 6", "1,2,3,4,6,8,12,24", ("proved","proved"), True),
    ("square36_pairs", {"kind":"factor_list","target":36},
     "1x36,2x18,3x12,4x9,6x6", "1,2,3,4,6,9,12,18,36", ("proved","proved"), True),
    ("invalid_pairs_complete_members", {"kind":"factor_list","target":6},
     "1x2,3x6", "1,2,3,6", ("disproved","proved"), True),
    ("missing_divisors", {"kind":"factor_list","target":24},
     "1,24", "1x24,2x12,3x8,4x6", ("disproved","proved"), True),
    ("rational_equal", {"kind":"numeric_expression","expression":"1/3+1/6"},
     "0.5", "2/4", ("proved","proved"), True),
    ("rational_unequal", {"kind":"numeric_expression","expression":"1/2"},
     "0.4", "0.5", ("disproved","proved"), True),
    ("polynomial_equal", {"kind":"polynomial_identity","expression":"(x+1)^2","variables":["x"]},
     "x^2+2*x+1", "(1+x)*(x+1)", ("proved","proved"), True),
    ("polynomial_counterexample", {"kind":"polynomial_identity","expression":"(x+1)^2","variables":["x"]},
     "x^2+1", "x^2+2*x+1", ("disproved","proved"), True),
    ("polynomial_grid_miss", {"kind":"polynomial_identity","expression":"0","variables":["x"]},
     "(x+2)*(x+1)*x*(x-1)*(x-2)", "0", ("unresolved","proved"), True),
    ("unsupported_variable_denominator", {"kind":"polynomial_identity","expression":"1/x","variables":["x"]},
     "1/x", "1/x", ("unresolved","unresolved"), False),
    ("unsupported_injected_answer", {"kind":"numeric_expression","expression":"1/2"},
     "__import__('os').system('id')", "1/2", ("unresolved","proved"), True),
]
def call(path, data=None):
    payload = None if data is None else json.dumps(data,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()
    request = urllib.request.Request("http://127.0.0.1:8080"+path, data=payload,
                                     headers={} if payload is None else {"Content-Type":"application/json"})
    with urllib.request.urlopen(request,timeout=115) as response:
        return json.load(response)

failures = 0
for attempt in range(30):
    try:
        health=call("/health")
        break
    except (OSError,urllib.error.URLError):
        time.sleep(1)
else:
    raise SystemExit("Verifier never became available")
print(json.dumps({"event":"health","response":health}),flush=True)
for name,problem,learner,candidate,wanted,supported in CASES:
    data={"source_hash":hashlib.sha256(name.encode()).hexdigest(),"problem":problem,
          "learner_answer":learner,"candidate_answer":candidate}
    before=time.monotonic()
    try:
        result=call("/verify",data)
        errors=[]
        if (result["learner"]["status"],result["candidate"]["status"])!=wanted:
            errors.append("verdict mismatch")
        if result["supported"]!=supported:
            errors.append("supported mismatch")
        completed=all(status in ("proved","disproved") for status in wanted)
        if result["status"]!=("completed" if completed else "unresolved"):
            errors.append("top-level status mismatch")
        if result["source_hash"]!=data["source_hash"]:
            errors.append("source binding mismatch")
        canonical=json.dumps(data,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()
        if result["evidence"].get("request_sha256")!=hashlib.sha256(canonical).hexdigest():
            errors.append("canonical UTF8 request binding mismatch")
        for role in ("learner","candidate"):
            if result[role]["status"] in ("proved","disproved"):
                steps=result["evidence"][role]["steps"]
                if [step["stage"] for step in steps]!=["compile","kernel_replay","statement_replay","replay_kernel"]:
                    errors.append("missing replay stages")
                if not all(step["ok"] for step in steps):
                    errors.append("accepted a failed stage")
        failures+=bool(errors)
        print(json.dumps({"event":"case","case":name,"seconds":round(time.monotonic()-before,3),
                          "errors":errors,"request":data,"response":result}),flush=True)
    except Exception as error:
        failures+=1
        print(json.dumps({"event":"case","case":name,"error":type(error).__name__+": "+str(error)}),flush=True)

# A fixed negative-control template confirms that incomplete Lean certificates
# cannot pass even if compilation would ordinarily only issue a warning.
from contracts import Claim
from lean_engine import Engine
claim=Claim("True","sorry","proved",("Mathlib.Tactic.NormNum",),{"test":"incomplete certificate"})
ok,message,evidence=Engine().verify(claim,time.monotonic()+50)
rejected_incomplete = not ok and any("sorry" in step.get("output","").lower() for step in evidence["steps"])
if not rejected_incomplete:
    failures+=1
print(json.dumps({"event":"incomplete_certificate_control","passed":rejected_incomplete,"message":message,"evidence":evidence}),flush=True)
print(json.dumps({"event":"summary","cases":len(CASES)+1,"failures":failures}),flush=True)
sys.exit(1 if failures else 0)
