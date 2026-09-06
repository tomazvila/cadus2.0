//! Generate zero-API review bundles against pending/approved rows of an explicit DB.
use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_store::{Db, DbConfig};
use cadus_worker::authoring::{
    cli::{AuthorArgs, select_for},
    completion::generate,
    job::{document_digest, served_instances},
    prompt::{AuthoringSpec, Kind},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

type Error = Box<dyn std::error::Error>;
type InventoryRow = (String, String, String, i64);
type Occupied = BTreeMap<(String, String), i64>;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let output = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("pass an output directory")?;
    complete(output).await
}

async fn complete(output: PathBuf) -> Result<(), Error> {
    let (curriculum, specs) = load_foundations()?;
    let db = Db::connect(&DbConfig::from_env()?).await?;
    let (rows, occupied) = inventory(&db).await?;
    let (drafts, review, kinds) = bundle(&curriculum, &specs, &db, &occupied).await?;
    db.pool().close().await;
    write_bundle(&output, &specs, &rows, &drafts, &review, &kinds)?;
    Ok(())
}

fn load_foundations() -> Result<(Curriculum, Vec<AuthoringSpec>), Error> {
    let root = std::env::var("CADUS_CURRICULUM").unwrap_or_else(|_| "curriculum".to_owned());
    let (curriculum, findings) = load_curriculum(std::path::Path::new(&root))?;
    if !findings.is_empty() {
        return Err(format!("curriculum has {} findings", findings.len()).into());
    }
    let specs = select_for(
        &curriculum,
        &AuthorArgs {
            course: Some("foundations".to_owned()),
            ..AuthorArgs::default()
        },
    )?;
    Ok((curriculum, specs))
}

async fn inventory(db: &Db) -> Result<(Vec<InventoryRow>, Occupied), Error> {
    let rows: Vec<InventoryRow> = sqlx::query_as(
        "SELECT kp_id,kind,status,count(*) FROM content_store GROUP BY kp_id,kind,status",
    )
    .fetch_all(db.pool())
    .await?;
    let mut occupied = Occupied::new();
    for (key, kind, status, count) in &rows {
        if status == "pending" || status == "approved" {
            *occupied.entry((key.clone(), kind.clone())).or_default() += count;
        }
    }
    Ok((rows, occupied))
}

async fn bundle(
    curriculum: &Curriculum,
    specs: &[AuthoringSpec],
    db: &Db,
    occupied: &Occupied,
) -> Result<(Vec<Value>, Vec<Value>, BTreeMap<String, usize>), Error> {
    let mut drafts = Vec::<Value>::new();
    let mut review = Vec::new();
    let mut kinds = BTreeMap::<String, usize>::new();
    for spec in specs {
        let (mut generated, row) = one_row(curriculum, spec, db, occupied, &mut kinds).await?;
        drafts.append(&mut generated);
        review.push(row);
    }
    Ok((drafts, review, kinds))
}

async fn one_row(
    curriculum: &Curriculum,
    spec: &AuthoringSpec,
    db: &Db,
    occupied: &Occupied,
    kinds: &mut BTreeMap<String, usize>,
) -> Result<(Vec<Value>, Value), Error> {
    let key = format!("{}/{}", spec.topic_id, spec.kp_id);
    let served = served_instances(db, &key).await?;
    let generated = generate(spec, &served);
    let mut drafts = Vec::new();
    let mut added = Vec::new();
    for draft in generated.drafts {
        let Some(kind) = draft["kind"].as_str() else {
            continue;
        };
        if occupied.contains_key(&(key.clone(), kind.to_owned())) {
            continue;
        }
        *kinds.entry(kind.to_owned()).or_default() += 1;
        let wire_kind = Kind::from_wire(kind).ok_or("unknown generated kind")?;
        added.push(json!({"kind":kind,"arguments_digest":document_digest(&key,wire_kind,&draft["arguments"].to_string())}));
        drafts.push(draft);
    }
    let topic = curriculum
        .topic(curriculum.idx_of(&spec.topic_id).ok_or("missing topic")?)
        .ok_or("missing topic")?;
    let diagnostic=topic.diagnostic_exemplar.as_ref().map(|item|json!({"problem":item.problem,"answer":item.answer,"answer_contract":item.answer_contract,"decidable":item.canonical_answer().is_ok()}));
    let row = json!({"kp_id":key,"objective":spec.kp_name,"source_answer_kind":spec.answer_kind,"constraints":spec.constraints,"source_exemplars":spec.exemplars.len(),"existing_instruction_template_counts":occupied.iter().filter(|((kp,_),_)|kp==&key).map(|((_,kind),count)|(kind.clone(),*count)).collect::<BTreeMap<_,_>>(),"new_drafts":added,"refusals":generated.refusals,"solution_proposals":generated.solutions,"assessment_proposals":generated.assessment,"diagnostic":diagnostic,"review_required":["pedagogical completeness","objective and operand bounds","template family variety","held-out family independence","human digest approval"]});
    Ok((drafts, row))
}

fn write_bundle(
    output: &PathBuf,
    specs: &[AuthoringSpec],
    rows: &[InventoryRow],
    drafts: &[Value],
    review: &[Value],
    kinds: &BTreeMap<String, usize>,
) -> Result<(), Error> {
    std::fs::create_dir_all(output)?;
    std::fs::write(
        output.join("drafts.json"),
        serde_json::to_string_pretty(drafts)?,
    )?;
    let solution_kps = review
        .iter()
        .filter(|row| {
            !row["solution_proposals"]
                .as_array()
                .is_none_or(Vec::is_empty)
        })
        .count();
    let assessment_kps = review
        .iter()
        .filter(|row| {
            !row["assessment_proposals"]
                .as_array()
                .is_none_or(Vec::is_empty)
        })
        .count();
    let summary = json!({"course":"foundations","knowledge_points":specs.len(),"drafts":drafts.len(),"new_drafts_by_kind":kinds,"solution_candidate_kps":solution_kps,"assessment_candidate_kps":assessment_kps,"source_inventory":rows,"pending_only":true,"approved_by_this_tool":0,"api_calls":0});
    std::fs::write(
        output.join("review.json"),
        serde_json::to_string_pretty(review)?,
    )?;
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_string_pretty(&summary)?,
    )?;
    let selected = drafts
        .iter()
        .filter_map(|row| row["kp_id"].as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let manifest = json!({"v":1,"course":"foundations","model":"operator-draft-v1","knowledge_points":selected,"kinds":["template","teach","hint_ladder"],"files":["drafts.json"],"review":"review.json","status":"pending-human-review"});
    std::fs::write(
        output.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}
