#!/usr/bin/env python
"""Canonical JSON dump of the 1.0 curriculum graph — the M1 parity oracle.

Usage: python dump_curriculum.py [CURRICULUM_DIR] > graph.json
"""
from __future__ import annotations
import hashlib, heapq, json, sys
sys.path.insert(0, "/home/deploy/dev/cadus")
from cadus.graph import load_curriculum


def exemplar(e):
    return None if e is None else {
        "problem": e.problem, "answer": e.answer, "solution_sketch": e.solution_sketch}


def edge(e):
    """One prerequisite or encompassing edge, as the dump records it."""
    return {"id": e.id, "weight": e.weight, "key": e.key}


def course_row(c):
    """One course of the catalog, as the dump records it."""
    return {"id": c.id, "name": c.name, "order": c.order,
            "mastery_floor": list(c.mastery_floor),
            "mastery_floor_course": c.mastery_floor_course}


def knowledge_point_row(kp):
    """One knowledge point of a topic, as the dump records it."""
    return {
        "id": kp.id, "name": kp.name,
        "key_prerequisites": list(kp.key_prerequisites),
        "constraints": kp.constraints,
        "exemplars": [exemplar(x) for x in kp.exemplars],
    }


def topic_row(g, tid, load_index):
    """One topic of the graph, as the dump records it."""
    t = g.topics[tid]
    return {
        "id": t.id, "name": t.name, "core": t.core,
        "difficulty": t.difficulty, "drill": t.drill,
        "answer_kind": str(t.answer_kind),
        "expected_time_secs": t.expected_time_secs,
        "course": g.topic_course[tid], "module": g.topic_module[tid],
        "unit": g.topic_unit[tid], "load_index": load_index[tid],
        "prerequisites": [edge(e) for e in t.prerequisites],
        "encompassings_extra": [edge(e) for e in t.encompassings_extra],
        "knowledge_points": [knowledge_point_row(kp) for kp in t.knowledge_points],
        "diagnostic_exemplar": exemplar(t.diagnostic_exemplar),
        "anki_seeds": [{"front": s.front, "back": s.back, "type": str(s.type)}
                       for s in t.anki_seeds],
    }


def topo_order(g, load_index):
    """Topological order: Kahn, ready set ordered by authored load_index."""
    indeg = {tid: len(g.prereqs[tid]) for tid in g.topics}
    ready = [load_index[t] for t in g.topics if indeg[t] == 0]
    heapq.heapify(ready)
    by_index = {i: t for t, i in load_index.items()}
    topo = []
    while ready:
        t = by_index[heapq.heappop(ready)]
        topo.append(t)
        for d in sorted(g.dependents[t]):
            indeg[d] -= 1
            if indeg[d] == 0:
                heapq.heappush(ready, load_index[d])
    return topo


def counts(g, courses, edges, enc):
    """The size of every part of the graph."""
    return {
        "courses": len(courses), "units": len(g.units), "topics": len(g.topics),
        "knowledge_points": sum(len(t.knowledge_points) for t in g.topics.values()),
        "exemplars": sum(len(kp.exemplars) for t in g.topics.values()
                         for kp in t.knowledge_points),
        "prereq_edges": len(edges), "encompassing_edges": len(enc),
        "anki_seeds": sum(len(t.anki_seeds) for t in g.topics.values()),
    }


def main(path: str) -> int:
    g = load_curriculum(path)
    load_index = {tid: i for i, tid in enumerate(g.topics)}  # authored order

    courses = [course_row(c) for c in g.catalog.courses]
    topics = [topic_row(g, tid, load_index) for tid in sorted(g.topics)]

    # Prerequisite edges: (child, parent) — child depends on parent. Sorted.
    edges = sorted(
        [tid, e.id, e.weight, e.key]
        for tid, t in g.topics.items() for e in t.prerequisites if e.id in g.topics
    )
    # Encompassing forward adjacency actually used by W() (prereqs + extras, max weight).
    enc = sorted([src, dst, w] for src, m in g._enc.items() for dst, w in m.items())

    out = {
        "schema": "cadus-curriculum-dump/1",
        "courses": courses,
        "topics": topics,
        "prereq_edges": edges,
        "encompassing_edges": enc,
        "topo_order": topo_order(g, load_index),
        "counts": counts(g, courses, edges, enc),
        "cycle": g.find_cycle(),
    }
    blob = json.dumps(out, sort_keys=True, ensure_ascii=False,
                      separators=(",", ":")).encode("utf-8")
    sys.stdout.write(blob.decode("utf-8"))
    sys.stdout.write("\n")
    print("sha256=" + hashlib.sha256(blob).hexdigest(), file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1] if len(sys.argv) > 1 else "curriculum"))
