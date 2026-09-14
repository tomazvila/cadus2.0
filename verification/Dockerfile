# Build context: verification/. No dependency installation occurs on the host.
FROM python:3.12.11-slim-bookworm@sha256:519591d6871b7bc437060736b9f7456b8731f1499a57e22e6c285135ae657bf7
ARG LEAN_VERSION=4.33.1
ARG LEAN_ARCHIVE_SHA256=890afd185370f85666025b883914ab4f4b339136f8c96167b69cfb62aecaf235
ARG MATHLIB_REVISION=0df444a360eaa60ab8c11dca51a86af692955474
RUN test "$(dpkg --print-architecture)" = amd64 \
    && apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl git zstd libgmp10 libstdc++6 \
    && rm -rf /var/lib/apt/lists/*
RUN curl --fail --location --retry 3 \
      "https://github.com/leanprover/lean4/releases/download/v$LEAN_VERSION/lean-$LEAN_VERSION-linux.tar.zst" \
      -o /tmp/lean.tar.zst \
    && printf '%s  %s\n' "$LEAN_ARCHIVE_SHA256" /tmp/lean.tar.zst | sha256sum --check - \
    && mkdir /opt/lean \
    && tar --zstd -xf /tmp/lean.tar.zst --strip-components=1 -C /opt/lean \
    && rm /tmp/lean.tar.zst \
    && test -x /opt/lean/bin/leanchecker
ENV PATH="/opt/lean/bin:$PATH" PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
RUN git init /opt/mathlib \
    && git -C /opt/mathlib remote add origin https://github.com/leanprover-community/mathlib4.git \
    && git -C /opt/mathlib fetch --depth=1 origin "$MATHLIB_REVISION" \
    && git -C /opt/mathlib checkout --detach FETCH_HEAD \
    && test "$(git -C /opt/mathlib rev-parse HEAD)" = "$MATHLIB_REVISION" \
    && test "$(cat /opt/mathlib/lean-toolchain)" = "leanprover/lean4:v$LEAN_VERSION"
WORKDIR /opt/mathlib
RUN lake exe cache get Mathlib/NumberTheory/Divisors.lean Mathlib/Tactic/NormNum.lean Mathlib/Tactic/Ring.lean \
    && lake env printenv LEAN_PATH > /opt/lean-path \
    && python3 -c "import pathlib; p=pathlib.Path('/opt/lean-path'); p.write_text(':'.join(str(pathlib.Path(x).resolve()) for x in p.read_text().strip().split(':')))" \
    && python3 -c "import subprocess; v=subprocess.check_output(['/opt/lean/bin/lean','--version'],text=True); assert 'version 4.33.1' in v, v" \
    && chmod -R a-w /opt/lean /opt/mathlib /opt/lean-path
RUN chmod -R a+rX /opt/lean /opt/mathlib /opt/lean-path
WORKDIR /app
COPY contracts.py lean_engine.py verify_service.py ./
USER 65532:65532
ENV VERIFIER_BIND=0.0.0.0 VERIFIER_PORT=8080
EXPOSE 8080
CMD ["python3", "-E", "-s", "-B", "/app/verify_service.py"]
