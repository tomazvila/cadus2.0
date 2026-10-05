//! Lesson-close scoring carried by durable, completed lesson proof chains.

use super::advance::Advance;
use super::{round2, *};
use cadus_store::proof_grading::JobRow;

/// A proof-chain penalty that belongs to the current unfinished lesson.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ProofScore {
    /// A revision or cap rewrite makes the lesson passable.
    pub(super) revised: bool,
    /// The unaided rewrite after the cap makes the lesson assisted.
    pub(super) assisted: bool,
}

impl ProofScore {
    fn include(&mut self, chain: &[JobRow]) {
        let Some(head) = chain.last() else {
            return;
        };
        self.revised |= chain[..chain.len() - 1]
            .iter()
            .any(|row| row.verdict().is_some())
            || head.rewrite;
        self.assisted |= head.rewrite;
    }
}

/// Read all completed lesson proof chains after this topic's latest lesson
/// result. The store query scopes roots to the unfinished lesson boundary and
/// returns full ancestry by chain id, so rollover does not lose the score.
pub(super) async fn completed(
    state: &AppState,
    tx: &mut Transaction<'static, Postgres>,
    topic: &str,
) -> Result<ProofScore, ApiError> {
    let heads = store(
        state,
        cadus_store::proof_grading::closed_lesson_heads_after_latest_result(&mut **tx, topic),
    )
    .await?;
    let mut score = ProofScore::default();
    for head in heads {
        if let Some(chain) = store(
            state,
            cadus_store::proof_grading::chain_rows(&mut **tx, head),
        )
        .await?
        {
            score.include(&chain);
        }
    }
    Ok(score)
}

/// Apply an earlier proof score to a passing lesson close before its event is
/// appended and projected.
pub(super) fn apply(
    graph: &Curriculum,
    cfg: &Config,
    mut moved: Advance,
    score: ProofScore,
) -> Advance {
    let Some(Event::LessonResult(result)) = moved.result.as_mut() else {
        return moved;
    };
    if !result.passed {
        return moved;
    }
    if score.revised {
        result.quality_tier = result.quality_tier.max(WorkQuality::Passable);
    }
    result.assisted |= score.assisted;
    let count = graph
        .idx_of(result.topic.as_str())
        .map_or(0, |idx| graph.knowledge_points(idx).len());
    let xp = task_xp(
        TaskType::Lesson,
        result.quality_tier,
        cfg,
        count as i64,
        0,
        false,
    );
    result.xp = xp;
    moved.xp = Some(round2(xp));
    moved
}
