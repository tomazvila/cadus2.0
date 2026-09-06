//! Generate zero-API review bundles against pending/approved rows of an explicit DB.
use cadus_core::{
    curriculum::{Curriculum, load_curriculum},
    instruction::template_instances,
};
use cadus_store::{Db, DbConfig};
use cadus_worker::authoring::{
    cli::{AuthorArgs, select_for},
    completion::{diagnosis_from_templates, generate},
    job::document_digest,
    prompt::{AuthoringSpec, Kind},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

type Error = Box<dyn std::error::Error>;
type InventoryRow = (String, String, String, i64);
type Occupied = BTreeMap<(String, String), i64>;
type TemplateBodies = BTreeMap<String, Vec<String>>;

struct Bundle {
    drafts: Vec<Value>,
    review: Vec<Value>,
    coverage: Vec<Value>,
    kinds: BTreeMap<String, usize>,
}

const COVERAGE_KINDS: [&str; 6] = [
    "teach",
    "hint_ladder",
    "solution_feedback",
    "template",
    "assessment",
    "diagnosis",
];

#[tokio::main]
async fn main() -> Result<(), Error> {
    let output = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: complete_foundations <output-dir> <authoritative-audit.json>")?;
    let audit = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .ok_or("usage: complete_foundations <output-dir> <authoritative-audit.json>")?;
    complete(output, &audit).await
}

async fn complete(output: PathBuf, audit: &Path) -> Result<(), Error> {
    let (curriculum, all_specs) = load_foundations()?;
    let clean = audit_clean_keys(audit, &all_specs)?;
    let specs: Vec<_> = all_specs
        .into_iter()
        .filter(|spec| clean.contains(&format!("{}/{}", spec.topic_id, spec.kp_id)))
        .collect();
    let db = Db::connect(&DbConfig::from_env()?).await?;
    let (rows, occupied, bodies) = inventory(&db).await?;
    let bundle = bundle(&curriculum, &specs, &occupied, &bodies)?;
    db.pool().close().await;
    write_bundle(&output, &specs, &rows, &bundle)?;
    Ok(())
}

fn audit_clean_keys(path: &Path, specs: &[AuthoringSpec]) -> Result<BTreeSet<String>, Error> {
    let report: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    if report["schema_version"] != 1 || report["course"] != "foundations" {
        return Err("audit report is not Foundations schema version 1".into());
    }
    if !report["orphan_pending_template_keys"]
        .as_array()
        .is_some_and(Vec::is_empty)
    {
        return Err("audit report contains orphan pending-template keys".into());
    }
    let rows = report["kps"]
        .as_array()
        .ok_or("audit report has no KP list")?;
    let expected: BTreeSet<_> = specs
        .iter()
        .map(|spec| format!("{}/{}", spec.topic_id, spec.kp_id))
        .collect();
    let actual: BTreeSet<_> = rows
        .iter()
        .filter_map(|row| row["kp_key"].as_str().map(str::to_owned))
        .collect();
    if actual != expected || actual.len() != rows.len() {
        return Err("audit report does not name every Foundations KP exactly once".into());
    }
    let clean = rows
        .iter()
        .filter(|row| row["issues"].as_array().is_some_and(Vec::is_empty))
        .filter_map(|row| row["kp_key"].as_str().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    if clean.is_empty() {
        return Err("audit report contains no clean Foundations KP".into());
    }
    Ok(clean)
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

async fn inventory(db: &Db) -> Result<(Vec<InventoryRow>, Occupied, TemplateBodies), Error> {
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
    let templates: Vec<(String, String)> = sqlx::query_as(
        "SELECT kp_id,body::text FROM content_store WHERE kind='template' \
         AND status IN ('pending','approved') ORDER BY kp_id,digest",
    )
    .fetch_all(db.pool())
    .await?;
    let mut bodies = TemplateBodies::new();
    for (key, body) in templates {
        bodies.entry(key).or_default().push(body);
    }
    Ok((rows, occupied, bodies))
}

fn bundle(
    curriculum: &Curriculum,
    specs: &[AuthoringSpec],
    occupied: &Occupied,
    bodies: &TemplateBodies,
) -> Result<Bundle, Error> {
    let mut drafts = Vec::<Value>::new();
    let mut review = Vec::new();
    let mut coverage = Vec::new();
    let mut kinds = BTreeMap::<String, usize>::new();
    for spec in specs {
        let (mut generated, row, mut covered) =
            one_row(curriculum, spec, occupied, bodies, &mut kinds)?;
        drafts.append(&mut generated);
        review.push(row);
        coverage.append(&mut covered);
    }
    Ok(Bundle {
        drafts,
        review,
        coverage,
        kinds,
    })
}

fn one_row(
    curriculum: &Curriculum,
    spec: &AuthoringSpec,
    occupied: &Occupied,
    bodies: &TemplateBodies,
    kinds: &mut BTreeMap<String, usize>,
) -> Result<(Vec<Value>, Value, Vec<Value>), Error> {
    let key = format!("{}/{}", spec.topic_id, spec.kp_id);
    let template_bodies = bodies.get(&key).map_or(&[][..], Vec::as_slice);
    let served: Vec<_> = template_bodies
        .iter()
        .flat_map(|body| template_instances(body))
        .collect();
    let mut generated = generate(spec, &served);
    if !occupied.contains_key(&(key.clone(), "diagnosis".to_owned()))
        && let Some(draft) = diagnosis_from_templates(spec, template_bodies)
    {
        generated.drafts.push(draft);
    }
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
    let existing = occupied
        .iter()
        .filter(|((kp, _), _)| kp == &key)
        .map(|((_, kind), count)| (kind.clone(), *count))
        .collect::<BTreeMap<_, _>>();
    let source_solutions: Vec<_> = spec.exemplars.iter().enumerate().map(|(index,item)|json!({"exemplar_index":index,"problem":item.problem,"answer":item.answer,"answer_contract":item.answer_contract,"solution_sketch":item.solution_sketch,"answer_decidable":item.canonical_answer().is_ok(),"audit_clean":true})).collect();
    let assessment = spec.exemplars.last().map(|item|json!({"problem":item.problem,"answer":item.answer,"answer_contract":item.answer_contract,"answer_decidable":item.canonical_answer().is_ok(),"source":"last decidable authored exemplar; held out by readiness"}));
    let row = json!({"kp_id":key,"objective":spec.kp_name,"source_answer_kind":spec.answer_kind,"constraints":spec.constraints,"source_exemplars":spec.exemplars.len(),"existing_instruction_template_counts":existing,"new_drafts":added,"refusals":generated.refusals,"solution_feedback_evidence":source_solutions,"derived_solution_proposals":generated.solutions,"assessment_evidence":assessment,"derived_assessment_proposals":generated.assessment,"diagnostic":diagnostic,"review_required":["pedagogical completeness","objective and operand bounds","template family variety","human digest approval"]});
    let coverage = coverage_rows(&row);
    Ok((drafts, row, coverage))
}

fn coverage_rows(row: &Value) -> Vec<Value> {
    let key = row["kp_id"].as_str().unwrap_or_default();
    COVERAGE_KINDS
        .iter()
        .map(|kind| coverage_row(key, kind, row))
        .collect()
}

fn coverage_row(key: &str, kind: &str, row: &Value) -> Value {
    if kind == "solution_feedback" {
        return evidence_coverage(key, kind, &row["solution_feedback_evidence"]);
    }
    if kind == "assessment" {
        let covered = row["assessment_evidence"].is_object();
        return coverage_value(key, kind, covered, "audit_clean_held_out_exemplar");
    }
    let existing = row["existing_instruction_template_counts"][kind]
        .as_i64()
        .unwrap_or(0);
    if existing > 0 {
        return json!({"kp_id":key,"kind":kind,"status":"covered","source":"content_store","count":existing});
    }
    if let Some(draft) = row["new_drafts"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["kind"] == kind))
    {
        return json!({"kp_id":key,"kind":kind,"status":"covered","source":"production_gated_pending_draft","arguments_digest":draft["arguments_digest"]});
    }
    let code = if kind == "diagnosis" {
        "no_explicit_wrong_answer_mapping"
    } else {
        "no_production_gated_candidate"
    };
    json!({"kp_id":key,"kind":kind,"status":"skipped","reason_code":code})
}

fn evidence_coverage(key: &str, kind: &str, evidence: &Value) -> Value {
    let covered = evidence.as_array().is_some_and(|items| {
        !items.is_empty()
            && items.iter().all(|item| {
                item["answer_decidable"] == true
                    && item["solution_sketch"]
                        .as_str()
                        .is_some_and(|text| !text.trim().is_empty())
            })
    });
    coverage_value(key, kind, covered, "audit_clean_authored_derivation")
}

fn coverage_value(key: &str, kind: &str, covered: bool, source: &str) -> Value {
    if covered {
        json!({"kp_id":key,"kind":kind,"status":"covered","source":source})
    } else {
        json!({"kp_id":key,"kind":kind,"status":"skipped","reason_code":"missing_verified_source_evidence"})
    }
}

fn validate_coverage(specs: &[AuthoringSpec], coverage: &[Value]) -> Result<(), Error> {
    let expected: BTreeSet<_> = specs
        .iter()
        .flat_map(|spec| {
            let key = format!("{}/{}", spec.topic_id, spec.kp_id);
            COVERAGE_KINDS
                .iter()
                .map(move |kind| (key.clone(), (*kind).to_owned()))
        })
        .collect();
    let mut actual = BTreeSet::new();
    for row in coverage {
        let key = row["kp_id"].as_str().ok_or("coverage row has no KP key")?;
        let kind = row["kind"].as_str().ok_or("coverage row has no kind")?;
        let valid_evidence = match row["status"].as_str() {
            Some("covered") => row["source"].as_str().is_some(),
            Some("skipped") => row["reason_code"].as_str().is_some(),
            _ => false,
        };
        if !valid_evidence {
            return Err(format!("{key}/{kind}: incomplete coverage evidence").into());
        }
        actual.insert((key.to_owned(), kind.to_owned()));
    }
    if actual != expected || actual.len() != coverage.len() {
        return Err("coverage report is not exactly six unique rows per eligible KP".into());
    }
    Ok(())
}

fn write_bundle(
    output: &PathBuf,
    specs: &[AuthoringSpec],
    rows: &[InventoryRow],
    bundle: &Bundle,
) -> Result<(), Error> {
    validate_coverage(specs, &bundle.coverage)?;
    std::fs::create_dir_all(output)?;
    std::fs::write(
        output.join("drafts.json"),
        serde_json::to_string_pretty(&bundle.drafts)?,
    )?;
    let solution_kps = bundle
        .review
        .iter()
        .filter(|row| {
            !row["solution_feedback_evidence"]
                .as_array()
                .is_none_or(Vec::is_empty)
        })
        .count();
    let assessment_kps = bundle
        .review
        .iter()
        .filter(|row| row["assessment_evidence"].is_object())
        .count();
    let mut coverage_counts = BTreeMap::<String, BTreeMap<String, usize>>::new();
    for item in &bundle.coverage {
        let kind = item["kind"].as_str().unwrap_or("invalid").to_owned();
        let status = item["status"].as_str().unwrap_or("invalid").to_owned();
        *coverage_counts
            .entry(kind)
            .or_default()
            .entry(status)
            .or_default() += 1;
    }
    let summary = json!({"course":"foundations","audit_clean_knowledge_points":specs.len(),"drafts":bundle.drafts.len(),"new_drafts_by_kind":&bundle.kinds,"solution_feedback_kps":solution_kps,"assessment_kps":assessment_kps,"coverage_by_kind":coverage_counts,"source_inventory":rows,"pending_only":true,"approved_by_this_tool":0,"api_calls":0});
    std::fs::write(
        output.join("review.json"),
        serde_json::to_string_pretty(&bundle.review)?,
    )?;
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_string_pretty(&summary)?,
    )?;
    std::fs::write(
        output.join("coverage.json"),
        serde_json::to_string_pretty(&json!({
            "schema_version":1,
            "course":"foundations",
            "eligible_knowledge_points":specs.len(),
            "expected_rows":specs.len() * COVERAGE_KINDS.len(),
            "rows":&bundle.coverage,
        }))?,
    )?;
    let selected = bundle
        .drafts
        .iter()
        .filter_map(|row| row["kp_id"].as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let manifest = json!({"v":1,"course":"foundations","model":"operator-draft-v1","knowledge_points":selected,"kinds":["template","teach","hint_ladder","diagnosis"],"files":["drafts.json"],"review":"review.json","coverage":"coverage.json","status":"pending-human-review"});
    std::fs::write(
        output.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}
