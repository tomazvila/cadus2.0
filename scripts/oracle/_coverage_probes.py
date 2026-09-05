"""The counting probes of `coverage_streams_1_0.py`.

`install` wraps the 1.0 FIRe functions and the 1.0 `Projector` hooks with
probes that record which branch a call took. A probe adds no state to the fold:
each wrapper calls the real function and records what that call did, so the
digests stay the digests of `digest_streams_1_0.py`.
"""

from __future__ import annotations

import math


class Probe:
    """The measured hit set of one stream."""

    def __init__(self) -> None:
        self.hits: set[str] = set()

    def hit(self, name: str) -> None:
        self.hits.add(name)


def close(a: float, b: float, tol: float = 1e-12) -> bool:
    """Whether two floats agree to within ``tol`` in absolute value."""
    return abs(a - b) <= tol


class FireProbes:
    """The probed FIRe functions of one stream.

    Each wrapper records the branch of the call and then calls the real function.
    The raw functions are read once, at construction, so `restore` puts the
    unwrapped 1.0 functions back.
    """

    def __init__(self, probe: Probe, fire_mod, topic_state) -> None:
        self.probe = probe
        self.fire = fire_mod
        self.TopicState = topic_state
        self.raw_raw_delta = fire_mod.raw_delta
        self.raw_decay_for = fire_mod.decay_for
        self.raw_interval_for = fire_mod.interval_for
        self.raw_speed_for = fire_mod.speed_for
        self.raw_apply_update = fire_mod._apply_update
        self.raw_apply_attempt = fire_mod.apply_attempt
        self.raw_memory_at = fire_mod.memory_at

    def _early_band(self, memory_now, config) -> None:
        """Record which band the pass `early` factor lands in."""
        span = 1.0 - config.fire.due_threshold
        if span <= 0.0:
            early = 1.0
        else:
            early = max(config.fire.early_floor, min((1.0 - memory_now) / span, 1.0))
        if close(early, config.fire.early_floor):
            self.probe.hit("fire.early_floor")
        elif close(early, 1.0):
            self.probe.hit("fire.early_clamp_1")
        else:
            self.probe.hit("fire.early_mid")

    def raw_delta(self, q, memory_now, passed, config, *, assisted=False):
        """Probed `raw_delta`: the early bands on a pass, the `q` edges on a miss."""
        if passed:
            self._early_band(memory_now, config)
        elif close(q, 0.0):
            self.probe.hit("fire.fail_q_0")
        elif close(q, 0.15):
            self.probe.hit("fire.fail_q_015")
        return self.raw_raw_delta(q, memory_now, passed, config, assisted=assisted)

    def decay_for(self, state, t, config):
        """Probed `decay_for`: the three bands of the overdue factor."""
        value = self.raw_decay_for(state, t, config)
        if close(value, 1.0):
            self.probe.hit("fire.decay_1")
        elif close(value, config.fire.decay_cap):
            self.probe.hit("fire.decay_cap_3")
        else:
            self.probe.hit("fire.decay_mid")
        return value

    def interval_for(self, rep_num, config):
        """Probed `interval_for`: the index bands of the table, and the cap."""
        table = config.fire.interval_table
        r = max(0.0, rep_num)
        last = len(table) - 1
        index = math.floor(r)
        if r <= 0.0:
            self.probe.hit("fire.interval_index_0")
        elif index >= last:
            self.probe.hit("fire.interval_last")
        else:
            self.probe.hit("fire.interval_interp")
        value = self.raw_interval_for(rep_num, config)
        if close(value, self.fire.INTERVAL_CAP_DAYS):
            self.probe.hit("fire.interval_cap_730")
        return value

    def speed_for(self, ability, difficulty, config):
        """Probed `speed_for`: the two clamps."""
        lo, hi = config.fire.speed_clamp
        rate = (0.5 + ability) / (0.5 + difficulty)
        if rate < lo:
            self.probe.hit("fire.speed_clamp_lo")
        elif rate > hi:
            self.probe.hit("fire.speed_clamp_hi")
        return self.raw_speed_for(ability, difficulty, config)

    def apply_update(self, state, raw, t, *, failed, cfg):
        """Probed `_apply_update`: the two floors at 0."""
        factor = self.raw_decay_for(state, t, cfg) if failed else 1.0
        if state.repNum + state.speed * factor * raw < 0.0:
            self.probe.hit("fire.repnum_floor_0")
        if self.raw_memory_at(state, t) + raw < 0.0:
            self.probe.hit("fire.membase_floor_0")
        return self.raw_apply_update(state, raw, t, failed=failed, cfg=cfg)

    def _scan_credit(self, states, graph, topic, grade, config, t, assisted) -> None:
        """Re-walk the downward credit gates of a pass."""
        for target, weight in sorted(graph.reach_weights(topic).items()):
            if target == topic or weight <= 0.0:
                continue
            recipient = states.get(target, self.TopicState())
            if recipient.speed < config.fire.explicit_speed_threshold:
                self.probe.hit("fire.forced_explicit_skip")
                continue
            credit = (
                self.raw_raw_delta(
                    grade,
                    self.raw_memory_at(recipient, t),
                    True,
                    config,
                    assisted=assisted,
                )
                * weight
            )
            if abs(credit) < config.fire.min_credit:
                self.probe.hit("fire.min_credit_drop")

    def _scan_penalty(self, states, graph, topic, raw, config) -> None:
        """Re-walk the upward penalty gates of a miss."""
        for target, weight in sorted(graph.upward_weights(topic).items()):
            if target == topic or weight <= 0.0:
                continue
            recipient = states.get(target, self.TopicState())
            if recipient.t0 is None:
                self.probe.hit("fire.t0_none_penalty_skip")
                continue
            if abs(raw * weight) < config.fire.min_credit:
                self.probe.hit("fire.min_credit_drop")

    def apply_attempt(self, states, attempt_result, graph, config, t):
        """Probed `apply_attempt`: the assisted flag and the propagation gates."""
        passed = attempt_result.passed
        if attempt_result.assisted:
            self.probe.hit("fire.assisted_pass" if passed else "fire.assisted_miss")
        topic = attempt_result.topic
        grade = self.fire.quality_q(attempt_result.quality)
        explicit = states.get(topic, self.TopicState())
        raw = self.raw_raw_delta(
            grade,
            self.raw_memory_at(explicit, t),
            passed,
            config,
            assisted=attempt_result.assisted,
        )
        # Re-walk the gates, because a DROPPED neighbor is absent from the report
        # the real function returns and must be measured here.
        if passed and raw > 0.0:
            self._scan_credit(states, graph, topic, grade, config, t, attempt_result.assisted)
        elif not passed and raw < 0.0:
            self._scan_penalty(states, graph, topic, raw, config)
        return self.raw_apply_attempt(states, attempt_result, graph, config, t)

    def install(self, proj_mod) -> None:
        """Put the wrappers on the `fire` module and on the projector's imports."""
        self.fire.raw_delta = self.raw_delta
        self.fire.decay_for = self.decay_for
        self.fire.interval_for = self.interval_for
        self.fire.speed_for = self.speed_for
        self.fire._apply_update = self.apply_update
        self.fire.apply_attempt = self.apply_attempt
        proj_mod.interval_for = self.interval_for
        proj_mod.speed_for = self.speed_for
        proj_mod.apply_attempt = self.apply_attempt

    def restore(self, proj_mod) -> None:
        """Put the unwrapped 1.0 functions back."""
        self.fire.raw_delta = self.raw_raw_delta
        self.fire.decay_for = self.raw_decay_for
        self.fire.interval_for = self.raw_interval_for
        self.fire.speed_for = self.raw_speed_for
        self.fire._apply_update = self.raw_apply_update
        self.fire.apply_attempt = self.raw_apply_attempt
        proj_mod.interval_for = self.raw_interval_for
        proj_mod.speed_for = self.raw_speed_for
        proj_mod.apply_attempt = self.raw_apply_attempt


class ProjectorProbes:
    """The probed `Projector` hooks of one stream.

    Each wrapper records the branch of the call and then calls the real hook.
    `projector` is the `Projector` instance the hook runs on.
    """

    def __init__(self, probe: Probe, proj_mod, topic_state, topic_status) -> None:
        self.probe = probe
        self.TopicState = topic_state
        self.TopicStatus = topic_status
        self.cls = proj_mod.Projector
        self.raw_placed = self.cls._on_diagnostic_placed
        self.raw_refresh = self.cls._refresh_placement
        self.raw_peel = self.cls._peel_back_conditional
        self.raw_reset = self.cls._on_profile_reset

    def placed(self, projector, event, apply_fire):
        """Probed `_on_diagnostic_placed`: initial or refresh, and the 0.0 boundary."""
        self.probe.hit("diag.refresh" if event.refresh else "diag.initial")
        if not event.refresh:
            # The INITIAL placement filter is `balance > 0.0` (projector.py:340-344),
            # a different guard from the refresh promote guard below. A `>=` port
            # folds identically unless some row sits exactly on 0.0.
            for tid, balance in event.balances.items():
                if tid in projector.graph.topics and balance == 0.0:
                    self.probe.hit("diag.placed_balance_zero")
        return self.raw_placed(projector, event, apply_fire)

    def refresh(self, projector, event, diag_answers):
        """Probed `_refresh_placement`: the H2 promote guard and its boundary."""
        for tid, balance in event.balances.items():
            if tid not in projector.graph.topics:
                continue
            old = projector.topics.get(tid, self.TopicState())
            if balance <= 0.0 and old.status is self.TopicStatus.untouched:
                # The boundary is recorded apart from the strictly-negative case: a
                # `balance < 0.0` guard folds identically to 1.0's `balance <= 0.0`
                # guard unless some row sits exactly on 0.0.
                self.probe.hit(
                    "diag.promote_guard_zero" if balance == 0.0 else "diag.promote_guard"
                )
        return self.raw_refresh(projector, event, diag_answers)

    def peel(self, projector, topic, t):
        """Probed `_peel_back_conditional`: a conditional candidate."""
        candidates = {topic} | projector.graph.dependents.get(topic, set())
        for cid in candidates:
            state = projector.topics.get(cid)
            if state is not None and state.conditional:
                self.probe.hit("diag.conditional_peel")
        return self.raw_peel(projector, topic, t)

    def reset(self, projector, event, apply_fire):
        """Probed `_on_profile_reset`: a reset that clears accumulated state."""
        if apply_fire:
            for tid in event.topics:
                if tid not in projector.graph.topics:
                    continue
                if projector.topics.get(tid, self.TopicState()) != self.TopicState():
                    self.probe.hit("reset.applied")
        return self.raw_reset(projector, event, apply_fire)

    def install(self) -> None:
        """Put the wrappers on the `Projector` class.

        A plain function on a class becomes a method and receives the instance;
        a bound method does not. So each wrapper is a function that passes the
        instance on.
        """
        hooks = self

        def placed(projector, event, apply_fire):
            return hooks.placed(projector, event, apply_fire)

        def refresh(projector, event, diag_answers):
            return hooks.refresh(projector, event, diag_answers)

        def peel(projector, topic, t):
            return hooks.peel(projector, topic, t)

        def reset(projector, event, apply_fire):
            return hooks.reset(projector, event, apply_fire)

        self.cls._on_diagnostic_placed = placed
        self.cls._refresh_placement = refresh
        self.cls._peel_back_conditional = peel
        self.cls._on_profile_reset = reset

    def restore(self) -> None:
        """Put the unwrapped 1.0 hooks back."""
        self.cls._on_diagnostic_placed = self.raw_placed
        self.cls._refresh_placement = self.raw_refresh
        self.cls._peel_back_conditional = self.raw_peel
        self.cls._on_profile_reset = self.raw_reset


def install(probe: Probe, cfg):
    """Wrap the 1.0 FIRe and projector entry points with the PROBES branch probes.

    Returns a no-argument ``restore`` function. The caller MUST call it after each
    stream. Without the restore, a second ``install`` wraps the first wrapper, and
    then every later fold also feeds the earlier stream's probe -- which reports
    one stream's branches as another stream's coverage.
    """
    from cadus import fire as fire_mod
    from cadus import projector as proj_mod
    from cadus.model import TopicState, TopicStatus

    fire = FireProbes(probe, fire_mod, TopicState)
    hooks = ProjectorProbes(probe, proj_mod, TopicState, TopicStatus)
    fire.install(proj_mod)
    hooks.install()

    def restore() -> None:
        """Put the unwrapped 1.0 functions back."""
        fire.restore(proj_mod)
        hooks.restore()

    return restore
