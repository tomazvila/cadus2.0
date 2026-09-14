"""Fresh bounded processes compile and replay application-owned proof templates."""
from dataclasses import dataclass
import hashlib
import os
from pathlib import Path
import re
import resource
import signal
import subprocess
import tempfile
import time

LEAN_VERSION = "4.33.1"
MATHLIB_REVISION = "0df444a360eaa60ab8c11dca51a86af692955474"
AXIOMS = frozenset(("propext","Classical.choice","Quot.sound"))
MAX_LOG = 65536

def digest(data):
    return hashlib.sha256(data.encode() if isinstance(data,str) else data).hexdigest()

def source_for(c):
    return ("\n".join("import " + x for x in c.imports) +
            "\nset_option autoImplicit false\nset_option linter.unusedVariables false\nset_option maxRecDepth 2048\nset_option maxHeartbeats 200000\n"
            "theorem CadusClaim : " + c.statement + " := by\n  " + c.tactic + "\n#print axioms CadusClaim\n")

def replay_for(c):
    return ("import CadusProof\nset_option autoImplicit false\nset_option linter.unusedVariables false\n"
            "theorem CadusReplay : " + c.statement + " := CadusClaim\n#print axioms CadusReplay\n")

def checked_axioms(output, name):
    if output.count("'" + name + "' does not depend on any axioms") == 1:
        return []
    matches = re.findall("'" + re.escape(name) + r"' depends on axioms:\s*\[([^\]]*)\]", output)
    if len(matches) != 1:
        raise ValueError("Missing or ambiguous axiom audit.")
    axioms = [x.strip() for x in matches[0].split(",") if x.strip()]
    if not set(axioms).issubset(AXIOMS):
        raise ValueError("Disallowed proof axiom.")
    return sorted(set(axioms))

@dataclass
class Engine:
    root: Path = Path("/opt/lean")
    path_file: Path = Path("/opt/lean-path")

    def available(self):
        return self.path_file.is_file() and all(os.access(self.root/"bin"/x,os.X_OK) for x in ("lean","leanchecker"))

    def metadata(self):
        return {"name":"Lean","version":LEAN_VERSION,"mathlib_revision":MATHLIB_REVISION,
                "template_version":"cadus-declarative-v1","axiom_allowlist":sorted(AXIOMS),
                "checker":"leanchecker","import_trust":"pinned application-owned mathlib cache","available":self.available()}

    def run(self, command, directory, deadline, label):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return {"ok":False,"reason":"request_deadline","output":""}
        env = {"PATH":str(self.root/"bin")+":/usr/bin:/bin","HOME":str(directory),
               "TMPDIR":str(directory),"LANG":"C.UTF-8",
               "LEAN_NUM_THREADS":"1","LEAN_STACK_SIZE_KB":"65536",
               "LEAN_PATH":str(directory)+":"+self.path_file.read_text().strip()}
        def limits():
            resource.setrlimit(resource.RLIMIT_CPU,(25,25))
            resource.setrlimit(resource.RLIMIT_AS,(8*1024**3,8*1024**3))
            resource.setrlimit(resource.RLIMIT_FSIZE,(16*1024**2,16*1024**2))
            resource.setrlimit(resource.RLIMIT_NOFILE,(64,64))
            resource.setrlimit(resource.RLIMIT_CORE,(0,0))
        log = directory/(label+".log")
        timed_out = False
        with log.open("wb") as out:
            p = subprocess.Popen(command,cwd=directory,env=env,stdin=subprocess.DEVNULL,
                                 stdout=out,stderr=subprocess.STDOUT,shell=False,
                                 start_new_session=True,preexec_fn=limits)
            try:
                p.wait(timeout=min(35,remaining))
            except subprocess.TimeoutExpired:
                timed_out = True
                os.killpg(p.pid,signal.SIGKILL)
                p.wait()
        with log.open("rb") as f:
            raw = f.read(MAX_LOG+1)
        text = raw[:MAX_LOG].decode("utf-8","replace")
        return {"ok":p.returncode==0 and not timed_out and len(raw)<=MAX_LOG and "warning:" not in text.lower(),
                "exit_code":p.returncode,"timed_out":timed_out,"output_truncated":len(raw)>MAX_LOG,
                "output":text,"output_sha256":digest(raw),"command":[str(x) for x in command]}

    def verify(self, claim, deadline):
        source,replay = source_for(claim),replay_for(claim)
        evidence = {"statement":claim.statement,"source":source,"source_sha256":digest(source),
                    "replay_source_sha256":digest(replay),"interpretation":claim.interpretation,"steps":[]}
        if not self.available():
            return False,"Pinned Lean compiler/checker unavailable.",evidence
        with tempfile.TemporaryDirectory(prefix="cadus-verify-") as tmp:
            directory = Path(tmp)
            proof,replay_file = directory/"CadusProof.lean",directory/"CadusReplay.lean"
            proof.write_text(source)
            replay_file.write_text(replay)
            lean,checker = self.root/"bin"/"lean",self.root/"bin"/"leanchecker"
            stages = [
                ("compile",[lean,"-j1","-s65536","-o",directory/"CadusProof.olean",proof],"CadusClaim"),
                ("kernel_replay",[checker,"CadusProof"],None),
                ("statement_replay",[lean,"-j1","-s65536","-o",directory/"CadusReplay.olean",replay_file],"CadusReplay"),
                ("replay_kernel",[checker,"CadusReplay"],None)]
            artifact_hash = None
            for label,command,name in stages:
                try:
                    step = self.run(command,directory,deadline,label)
                except (OSError,ValueError) as error:
                    return False,"Checker infrastructure unavailable: "+type(error).__name__,evidence
                step["stage"] = label
                evidence["steps"].append(step)
                if not step["ok"]:
                    return False,"Formal verification did not complete: "+label,evidence
                if name:
                    try:
                        step["axioms"] = checked_axioms(step["output"],name)
                    except ValueError as error:
                        return False,str(error),evidence
                if label == "compile":
                    artifact_hash = digest((directory/"CadusProof.olean").read_bytes())
                if proof.read_text()!=source or replay_file.read_text()!=replay:
                    return False,"Trusted template bytes changed.",evidence
                if artifact_hash != digest((directory/"CadusProof.olean").read_bytes()):
                    return False,"Compiled proof changed during replay.",evidence
            evidence["proof_olean_sha256"] = artifact_hash
        return True,"Kernel certificate and fixed-statement replay accepted.",evidence
