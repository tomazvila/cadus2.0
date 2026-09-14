# Declarative formal verification
A private stdlib HTTP service verifies bounded mathematical claims using application-owned Lean templates. It has no model client, database connection, repair executor, or publication authority.
## Request and response
POST /verify, Content-Type application/json, maximum16384bytes:
~~~json
{"source_hash":"sha256:caller-bound-source","problem":{"kind":"factor_list","target":12},"learner_answer":"1x12,2x6,3x4","candidate_answer":"1,2,3,4,6,12"}
~~~
Other problem shapes:
~~~json
{"kind":"numeric_expression","expression":"(1/3+1/6)^2"}
{"kind":"polynomial_identity","expression":"(x+1)^2","variables":["x"]}
~~~
Each arithmetic answer is an expression compared to the problem expression. Polynomial equality means equality for all rational assignments of the declared variables.
~~~json
{"source_hash":"sha256:caller-bound-source","status":"completed","learner":{"status":"proved","message":"..."},"candidate":{"status":"proved","message":"..."},"engine":{"name":"Lean","version":"4.33.1","available":true},"evidence":{"request_sha256":"...","learner":{"statement":"...","source":"...","source_sha256":"...","steps":[]},"candidate":{"statement":"...","source":"...","source_sha256":"...","steps":[]}},"supported":true}
~~~
Each answer is proved, disproved, or unresolved. Completed requires both certificates to finish. Disproved requires a checked negation/counterexample; failure to prove never establishes falsehood. Unsupported notation, missing Lean, warnings, timeouts, axiom violations, and replay failures are unresolved. Supported describes the problem contract, not whether every submitted answer notation parses.
Malformed HTTP/JSON gives4xx; unsupported mathematics givesHTTP200 with unresolved results. GET /health checks executable/cache availability; it is not a formal smoke test.
The caller must construct the problem from trusted application content and bind source_hash. Echoing a supplied hash cannot establish that a model correctly transcribed the external question. evidence.request_sha256 hashes UTF8 Python json.dumps(request,sort_keys=True,separators=(",",":"),ensure_ascii=False).
## Formal trust boundary
Only a bounded arithmetic AST or integer-list/pair parser can feed the renderer. Raw strings cannot choose Lean imports, statements outside those templates, tactics, commands, files, or modules.
Each answer uses fresh processes to compile CadusClaim, replay CadusProof with leanchecker, compile a separately rendered CadusReplay against the identical expected statement, and replay that module. Source and compiled-proof hashes must remain unchanged.
The standard axiom allowlist is propext, Classical.choice, and Quot.sound. Missing/ambiguous axiom audits, sorryAx, native/compiler axioms, custom axioms, or compiler warnings block success.
Factors use finite-set equality with Nat.divisors and positivity/product checks for EVERY written pair, proved by decide +kernel. No native_decide is used. Rational equations and negations use norm_num. Polynomial equality uses ring; easy false claims use a checked negated universal theorem instantiated at a rational witness.
Python arithmetic only selects a proof direction or suggests a counterexample. It is never formal evidence.
Fresh checker processes replay request modules while trusting pinned mathlib imports. They do not recheck every library dependency on each request, and are not independently implemented external kernels. Lean, mathlib definitions, the parser/renderer, source binding, and isolation remain trusted components.
## Supported data and bounds
- Factors: positive target1..4096; maximum128 submitted integers; flat comma/semicolon/whitespace lists and integer pairs using x/X/*, multiplication sign, middle dot, times, or LaTeX \times. Separate pair groups with comma, semicolon, and, or newline. Every written pair must be positive and multiply to the target. Set order and duplicates do not matter.
- Arithmetic: exact integer/decimal literals, explicit + - * / ^ **, parentheses, declared single-letter variables, and exponents0..12. Denominators must be constant and nonzero. No implicit multiplication, arbitrary functions, LaTeX arithmetic, negative powers, scientific notation, equations, variable-denominator functions, or notation requirements.
- Maximum512expression characters,128AST nodes, nesting24,4variables, literal magnitude1e9, rational numerator/denominator2048bits.
- Counterexample search has at most625 assignments from {-2,-1,0,1,2}. A false polynomial missed by this grid remains unresolved when ring fails.
- Serial processing bounds fan-out. Each request has100seconds overall. Each child uses one Lean worker with a64MiB thread stack and has35seconds wall,25CPU seconds,8GiB virtual address space (separate from the 3GiB container RAM cap),16MiB file-size,64descriptors, no core dumps, and64KiB captured output. Temporary files and timed-out process groups are cleaned.
## Image and deployment handoff
Lean and mathlib matching stable4.33.1 were released August21,2026. Lean AMD64 archive checksum:890afd185370f85666025b883914ab4f4b339136f8c96167b69cfb62aecaf235. Mathlib commit:0df444a360eaa60ab8c11dca51a86af692955474. The Python3.12.11 base has an OCI digest pin; transitive mathlib dependencies use the committed lake manifest. OS package repositories and upstream cache transport remain build-time dependencies, so complete byte reproducibility is not claimed.
Build only after resource approval:
~~~sh
docker build -t cadus-verifier:lean4.33.1 verification/
~~~
Linux AMD64 only. Downloads occur during build. Runtime uses cached local modules and never invokes Lake, Git, package managers, model APIs, or user code.
Connect only to an internal network with the trusted caller; publish no host port. Enforce read-only root, non-root UID, no capabilities, no-new-privileges, PID/memory/CPU caps, size-limited temporary storage, and network-egress denial. No host mounts, Docker socket, SSH keys, model keys, DB credentials, or provider proxies may enter the service.
~~~yaml
read_only: true
user: "65532:65532"
cap_drop: ["ALL"]
security_opt: ["no-new-privileges:true"]
pids_limit: 64
mem_limit: 3g
cpus: 1
tmpfs:
  - /tmp:rw,noexec,nosuid,size=256m,mode=1777
~~~
Docker shares the host kernel; stronger sandboxing can be layered on by the operator. No configuration outside this directory is changed.
## Validation handoff
~~~sh
cd verification
python3 -m unittest -v test_verification
~~~
Unit tests cover parsers and status boundaries using declining/missing engine fixtures. Run integration_tests.py inside an isolated running image to exercise HTTP plus real Lean certificates.
The September14,2026 focused gate passed11 unit tests and12 integration controls. Real certificates covered factor24 pairs, square36, false written pairs, missing divisors, equal/unequal rationals, polynomial identities and checked counterexamples. Additional controls preserved unresolved status for a missed polynomial counterexample, refused variable denominators/code-like input, and rejected an actual Lean sorry certificate. Every accepted answer completed both compilation and both kernel replay stages; canonical UTF8 request hashes matched.
Tested image: cadus-verifier:report-dev-20260914, sha256:b283a079ec99dda165150f0380cd160c7ac706bae03169693c64d9c1606ea238. Tests used network none, read-only UID65532,2CPUs,3GiB RAM,64PIDs, dropped capabilities and256MiB noexec tmpfs. Successful two-answer requests took16.4-17.5seconds. Test containers were removed; the image was retained. Evidence logs are in /home/deploy/.cache/cadus-verifier-20260914/.
The initial gates exposed and repaired cache read permissions, Lean virtual-address/thread limits, and a harmless unused-variable style warning. The fixed template suppresses only that style linter. Other warnings and disallowed axioms still prevent success.
Production deployment, full supported-boundary load tests, adversarial resource-exhaustion tests, and end-to-end model/source transcription remain separate gates. GET /health is serialized with verification and may wait behind a request.
## Official sources
- https://github.com/leanprover-community/mathlib4/releases/tag/v4.33.1
- https://github.com/leanprover-community/mathlib4/blob/0df444a360eaa60ab8c11dca51a86af692955474/lean-toolchain
- https://lean-lang.org/doc/reference/latest/ValidatingProofs/
- https://github.com/leanprover/lean4checker
