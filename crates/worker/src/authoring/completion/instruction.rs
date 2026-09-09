//! Objective-and-constraints-pinned instruction ladders for canonical gaps.
use super::{Proposals, keep};
use crate::authoring::prompt::{AuthoringSpec, Kind};
use cadus_core::instruction::ServedInstance;
use serde::Deserialize;
use serde_json::json;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct HintRecipe {
    key: String,
    objective: String,
    constraints: String,
    hints: Vec<String>,
}

#[allow(clippy::expect_used)]
fn hints() -> &'static [HintRecipe] {
    static HINTS: OnceLock<Vec<HintRecipe>> = OnceLock::new();
    HINTS.get_or_init(|| {
        let mut catalog: Vec<HintRecipe> = serde_json::from_str(include_str!("instruction_hints.json"))
            .expect("the reviewed instruction-hint catalog is valid");
        for shard in [include_str!("instruction_hints_repairs.json")] {
            catalog.extend(serde_json::from_str::<Vec<HintRecipe>>(shard)
                .expect("the reviewed instruction-hint shard is valid"));
        }
        catalog
    })
}

pub(super) fn add_reviewed_hint(
    out: &mut Proposals,
    spec: &AuthoringSpec,
    served: &[ServedInstance],
) {
    if out.drafts.iter().any(|row| row["kind"] == "hint_ladder") {
        return;
    }
    let key = format!("{}/{}", spec.topic_id, spec.kp_id);
    let Some(recipe) = hints().iter().find(|item| {
        item.key == key
            && item.objective == spec.kp_name
            && spec.constraints.as_deref() == Some(item.constraints.as_str())
    }) else {
        return;
    };
    keep(
        out,
        spec,
        Kind::HintLadder,
        json!({"hints": recipe.hints}),
        served,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authoring::cli::select;
    use cadus_core::curriculum::load_curriculum;
    use std::{collections::BTreeSet, path::Path};

    fn specs() -> Vec<AuthoringSpec> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
        let (curriculum, findings) = load_curriculum(&root).expect("curriculum");
        assert!(findings.is_empty());
        let keys: Vec<_> = hints().iter().map(|row| row.key.clone()).collect();
        select(&curriculum, &keys).expect("all pinned hint KPs")
    }

    #[test]
    fn catalog_is_exact_unique_and_gated() {
        let keys: BTreeSet<_> = hints().iter().map(|row| row.key.as_str()).collect();
        assert_eq!(keys.len(), 28);
        assert_eq!(keys.len(), hints().len());
        for spec in specs() {
            let mut out = Proposals::default();
            add_reviewed_hint(&mut out, &spec, &[]);
            assert_eq!(out.drafts.len(), 1, "{}", spec.topic_id);
        }
    }

    #[test]
    fn metadata_drift_cannot_reuse_a_ladder() {
        for spec in specs() {
            for objective in [false, true] {
                let mut changed = spec.clone();
                if objective {
                    changed.kp_name.push_str(" changed");
                } else {
                    changed.constraints = Some("changed bounds".into());
                }
                let mut out = Proposals::default();
                add_reviewed_hint(&mut out, &changed, &[]);
                assert!(out.drafts.is_empty(), "{}", spec.topic_id);
            }
        }
    }
}
