//! Placement-diagnostic event handlers.

use std::collections::{BTreeMap, BTreeSet};

use crate::event::{DiagnosticAnswer, DiagnosticPlaced, Slug, Timestamp, TopicStatus};
use crate::fire::{difficulty, initial_ability, interval_for, py_min, speed_for};

use super::super::{
    ABILITY_SEED_PRIOR, INFERRED_SEED_BIAS, PLACEMENT_MEMORY_BASE, PLACEMENT_REPNUM_CAP, Projector,
    clamp01, refreshed_repnum,
};

impl Projector<'_> {
    /// `diagnostic_answer` (`projector.py:289-299`). A light index: tallied on every
    /// replay, so an incrementally applied placement sees the answers before it.
    pub(in crate::projector) fn on_diagnostic_answer(&mut self, event: &DiagnosticAnswer) {
        if event
            .outcome
            .as_ref()
            .is_some_and(|outcome| outcome.is_ungraded())
        {
            return;
        }
        self.diag_answers
            .entry(event.topic.as_str().to_owned())
            .or_default()
            .push((event.correct, event.weight.get()));
    }

    /// `diagnostic_placed` (`projector.py:301-371`).
    ///
    /// The per-session answer tally is consumed AND reset UNCONDITIONALLY, ahead of the
    /// `apply_fire` gate, so a light replay and a full replay stay consistent.
    pub(in crate::projector) fn on_diagnostic_placed(
        &mut self,
        event: &DiagnosticPlaced,
        ts: i64,
        apply_fire: bool,
    ) {
        let diag_answers = std::mem::take(&mut self.diag_answers);
        if !apply_fire {
            return;
        }
        if event.refresh {
            self.refresh_placement(event, ts, &diag_answers);
            return;
        }
        let graph = self.graph;
        let conditional: BTreeSet<&str> = event.conditional.iter().map(Slug::as_str).collect();
        // `balances` keeps the JSON key order of 1.0's dict (trap T6).
        let placed: Vec<(String, f64)> = event
            .balances
            .iter()
            .filter(|(tid, balance)| graph.idx_of(tid.as_str()).is_some() && **balance > 0.0)
            .map(|(tid, balance)| (tid.clone(), *balance))
            .collect();
        let answered: BTreeSet<&str> = placed
            .iter()
            .filter(|(tid, _)| {
                diag_answers
                    .get(tid)
                    .is_some_and(|answers| !answers.is_empty())
            })
            .map(|(tid, _)| tid.as_str())
            .collect();

        // Pass 1: a directly answered topic seeds from its own correct/speed evidence.
        for (tid, balance) in &placed {
            if !answered.contains(tid.as_str()) {
                continue;
            }
            let ability = diag_answers.get(tid).map_or(ABILITY_SEED_PRIOR, |answers| {
                self.ability_from_answers(answers)
            });
            self.seed_placed(tid, *balance, ability, ts, &conditional);
        }
        // Pass 2: an inferred topic falls back to the now-populated neighborhood, low.
        for (tid, balance) in &placed {
            if answered.contains(tid.as_str()) {
                continue;
            }
            let nbr = initial_ability(tid, graph, &self.topics, self.cfg);
            self.seed_placed(
                tid,
                *balance,
                clamp01(nbr * INFERRED_SEED_BIAS),
                ts,
                &conditional,
            );
        }
    }

    /// Place one topic at `balance` with the given ability (`projector.py:345-360`).
    fn seed_placed(
        &mut self,
        tid: &str,
        balance: f64,
        ability: f64,
        ts: i64,
        conditional: &BTreeSet<&str>,
    ) {
        let rep = py_min(balance, PLACEMENT_REPNUM_CAP);
        let mut state = self.topics.get(tid).cloned().unwrap_or_default();
        state.status = TopicStatus::Placed;
        state.rep_num = rep;
        state.memory_base = PLACEMENT_MEMORY_BASE;
        state.t0 = Some(Timestamp::from_micros(ts));
        state.interval_days = interval_for(rep, self.cfg);
        state.ability = ability;
        state.speed = speed_for(ability, difficulty(self.graph, tid), self.cfg);
        state.conditional = conditional.contains(tid);
        self.topics.insert(tid.to_owned(), state);
    }

    /// A REFRESH diagnostic (`projector.py:373-422`).
    ///
    /// A refresh AVERAGES its evidence into the current state, so it also DEMOTES
    /// a stale topic. The H2 guard keeps it from PROMOTING never-learned material: a
    /// non-positive balance on an untouched topic is skipped.
    fn refresh_placement(
        &mut self,
        event: &DiagnosticPlaced,
        ts: i64,
        diag_answers: &BTreeMap<String, Vec<(bool, f64)>>,
    ) {
        let graph = self.graph;
        let conditional: BTreeSet<&str> = event.conditional.iter().map(Slug::as_str).collect();
        // `balances` keeps the JSON key order of 1.0's dict (trap T6).
        let rows: Vec<(String, f64)> = event
            .balances
            .iter()
            .map(|(tid, balance)| (tid.clone(), *balance))
            .collect();
        for (tid, balance) in rows {
            if graph.idx_of(tid.as_str()).is_none() {
                continue;
            }
            let old = self.topics.get(&tid).cloned().unwrap_or_default();
            if balance <= 0.0 && old.status == TopicStatus::Untouched {
                continue;
            }
            let new_rep = refreshed_repnum(old.rep_num, balance);
            // `on_diagnostic_answer` creates a list with its first entry, so a
            // present list holds at least one answer.
            let ability = match diag_answers.get(&tid) {
                Some(answers) => {
                    let fresh = self.ability_from_answers(answers);
                    clamp01((old.ability + fresh) / 2.0)
                }
                None => old.ability,
            };
            let mut next = old;
            next.status = TopicStatus::Placed;
            next.rep_num = new_rep;
            next.memory_base = PLACEMENT_MEMORY_BASE;
            next.t0 = Some(Timestamp::from_micros(ts));
            next.interval_days = interval_for(new_rep, self.cfg);
            next.ability = ability;
            next.speed = speed_for(ability, difficulty(graph, &tid), self.cfg);
            next.conditional = conditional.contains(tid.as_str());
            self.topics.insert(tid, next);
        }
    }
}
