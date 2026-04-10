use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context, bail};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{storage::vector::VectorStorage, types::Drawer};

const METRIC_CUTOFFS: [usize; 6] = [1, 3, 5, 10, 30, 50];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LongMemEvalGranularity {
    Session,
    Turn,
}

impl LongMemEvalGranularity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Turn => "turn",
        }
    }
}

impl FromStr for LongMemEvalGranularity {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "session" => Ok(Self::Session),
            "turn" => Ok(Self::Turn),
            other => bail!("unsupported granularity: {other}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LongMemEvalBenchmarkOptions {
    pub granularity: LongMemEvalGranularity,
    pub max_questions: Option<usize>,
    pub output_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LongMemEvalBenchmarkRun {
    pub summary: LongMemEvalBenchmarkSummary,
    pub results: Vec<LongMemEvalBenchmarkResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LongMemEvalBenchmarkSummary {
    pub dataset_path: String,
    pub granularity: LongMemEvalGranularity,
    pub total_questions: usize,
    pub evaluated_questions: usize,
    pub skipped_abstention_questions: usize,
    pub skipped_no_target_questions: usize,
    pub metrics: BTreeMap<String, BTreeMap<String, f64>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LongMemEvalBenchmarkResult {
    pub question_id: String,
    pub question_type: String,
    pub question: String,
    pub answer: String,
    pub question_date: String,
    pub answer_session_ids: Vec<String>,
    pub retrieval_results: LongMemEvalRetrievalResult,
    #[serde(skip)]
    is_abstention: bool,
    #[serde(skip)]
    has_target_user_turn: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LongMemEvalRetrievalResult {
    pub query: String,
    pub corpus_size: usize,
    pub ranked_items: Vec<LongMemEvalRankedItem>,
    pub metrics: BTreeMap<String, BTreeMap<String, f64>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LongMemEvalRankedItem {
    pub corpus_id: String,
    pub text: String,
    pub timestamp: String,
    pub similarity: f32,
}

#[derive(Debug, Clone, Deserialize)]
struct LongMemEvalEntry {
    question_id: String,
    question_type: String,
    question: String,
    answer: String,
    question_date: String,
    haystack_session_ids: Vec<String>,
    haystack_dates: Vec<String>,
    haystack_sessions: Vec<Vec<LongMemEvalTurn>>,
    answer_session_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LongMemEvalTurn {
    role: String,
    content: String,
    #[serde(default)]
    has_answer: Option<bool>,
}

#[derive(Debug, Clone)]
struct CorpusItem {
    id: String,
    text: String,
    timestamp: String,
    chunk_index: i64,
}

pub struct LongMemEvalBenchmark {
    embedding: TextEmbedding,
}

pub fn run_cli(args: Vec<String>) -> anyhow::Result<()> {
    let Some(options) = parse_cli_args(args)? else {
        return Ok(());
    };
    let mut benchmark = LongMemEvalBenchmark::new()?;
    let benchmark_run = benchmark.run_path(&options.data_path, &options.benchmark_options)?;
    println!("{}", serde_json::to_string_pretty(&benchmark_run.summary)?);
    Ok(())
}

impl LongMemEvalBenchmark {
    pub fn new() -> anyhow::Result<Self> {
        let embedding = TextEmbedding::try_new(InitOptions::new(EmbeddingModel::AllMiniLML6V2))
            .map_err(|err| anyhow::anyhow!("{err}"))
            .context("failed to initialize embedding model")?;
        Ok(Self { embedding })
    }

    pub fn run_path(
        &mut self,
        data_path: &Path,
        options: &LongMemEvalBenchmarkOptions,
    ) -> anyhow::Result<LongMemEvalBenchmarkRun> {
        let file = File::open(data_path).with_context(|| {
            format!(
                "failed to open LongMemEval dataset at {}",
                data_path.display()
            )
        })?;
        let mut entries: Vec<LongMemEvalEntry> =
            serde_json::from_reader(file).with_context(|| {
                format!(
                    "failed to parse LongMemEval dataset at {}",
                    data_path.display()
                )
            })?;

        if let Some(max_questions) = options.max_questions {
            entries.truncate(max_questions);
        }

        let mut results = Vec::with_capacity(entries.len());
        for entry in &entries {
            results.push(self.run_entry(entry, options.granularity)?);
        }

        if let Some(output_path) = options.output_path.as_deref() {
            write_results_jsonl(output_path, &results)?;
        }

        let summary = aggregate_results(&results, data_path, options.granularity);
        Ok(LongMemEvalBenchmarkRun { summary, results })
    }

    fn run_entry(
        &mut self,
        entry: &LongMemEvalEntry,
        granularity: LongMemEvalGranularity,
    ) -> anyhow::Result<LongMemEvalBenchmarkResult> {
        let corpus_items = build_corpus_items(entry, granularity);
        let temp_dir = create_temp_benchmark_dir()?;
        let db_path = temp_dir.join("palace.sqlite3");
        let result = self.run_entry_with_db(entry, granularity, &corpus_items, &db_path);
        let _ = std::fs::remove_dir_all(&temp_dir);
        result
    }

    fn run_entry_with_db(
        &mut self,
        entry: &LongMemEvalEntry,
        granularity: LongMemEvalGranularity,
        corpus_items: &[CorpusItem],
        db_path: &Path,
    ) -> anyhow::Result<LongMemEvalBenchmarkResult> {
        if corpus_items.is_empty() {
            return Ok(LongMemEvalBenchmarkResult {
                question_id: entry.question_id.clone(),
                question_type: entry.question_type.clone(),
                question: entry.question.clone(),
                answer: entry.answer.clone(),
                question_date: entry.question_date.clone(),
                answer_session_ids: entry.answer_session_ids.clone(),
                retrieval_results: LongMemEvalRetrievalResult {
                    query: entry.question.clone(),
                    corpus_size: 0,
                    ranked_items: Vec::new(),
                    metrics: empty_metrics(granularity),
                },
                is_abstention: entry.question_id.contains("_abs"),
                has_target_user_turn: has_target_user_turn(entry),
            });
        }

        let storage = VectorStorage::new(db_path).with_context(|| {
            format!("failed to initialize benchmark DB at {}", db_path.display())
        })?;

        let texts: Vec<&str> = corpus_items.iter().map(|item| item.text.as_str()).collect();
        let embeddings = self
            .embedding
            .embed(texts, None)
            .map_err(|err| anyhow::anyhow!("{err}"))
            .context("failed to embed LongMemEval history sessions")?;

        for (item, embedding) in corpus_items.iter().zip(embeddings.iter()) {
            let drawer = Drawer {
                id: item.id.clone(),
                content: item.text.clone(),
                wing: entry.question_type.clone(),
                room: item.id.clone(),
                source_file: "longmemeval".to_string(),
                source_mtime: 0,
                chunk_index: item.chunk_index,
                added_by: "benchmark".to_string(),
                filed_at: item.timestamp.clone(),
                hall: String::new(),
                topic: entry.question_id.clone(),
                drawer_type: granularity.as_str().to_string(),
                agent: "longmemeval".to_string(),
                date: entry.question_date.clone(),
                importance: 1.0,
            };
            storage
                .add_drawer(&drawer, embedding)
                .with_context(|| format!("failed to add benchmark drawer {}", item.id))?;
        }

        let mut query_embedding = self
            .embedding
            .embed(vec![entry.question.as_str()], None)
            .map_err(|err| anyhow::anyhow!("{err}"))
            .context("failed to embed LongMemEval query")?;
        let query_embedding = query_embedding
            .pop()
            .context("embedding model returned no query vector")?;

        let search_results = storage
            .search(&query_embedding, corpus_items.len(), None, None)
            .context("failed to search benchmark corpus")?;

        let mut items_by_id = BTreeMap::new();
        for item in corpus_items {
            items_by_id.insert(item.id.clone(), item);
        }

        let ranked_ids: Vec<String> = search_results
            .iter()
            .map(|result| result.drawer.id.clone())
            .collect();
        let correct_docs: Vec<String> = corpus_items
            .iter()
            .filter(|item| item.id.contains("answer"))
            .map(|item| item.id.clone())
            .collect();

        let mut metrics = BTreeMap::new();
        let mut primary_metrics = BTreeMap::new();
        for cutoff in METRIC_CUTOFFS {
            let (recall_any, recall_all, ndcg_any) =
                evaluate_retrieval(&ranked_ids, &correct_docs, cutoff);
            primary_metrics.insert(format!("recall_any@{cutoff}"), recall_any);
            primary_metrics.insert(format!("recall_all@{cutoff}"), recall_all);
            primary_metrics.insert(format!("ndcg_any@{cutoff}"), ndcg_any);
        }
        metrics.insert(granularity.as_str().to_string(), primary_metrics);

        if granularity == LongMemEvalGranularity::Turn {
            let mut session_metrics = BTreeMap::new();
            for cutoff in METRIC_CUTOFFS {
                let (recall_any, recall_all, ndcg_any) =
                    evaluate_retrieval_turn_to_session(&ranked_ids, &correct_docs, cutoff);
                session_metrics.insert(format!("recall_any@{cutoff}"), recall_any);
                session_metrics.insert(format!("recall_all@{cutoff}"), recall_all);
                session_metrics.insert(format!("ndcg_any@{cutoff}"), ndcg_any);
            }
            metrics.insert(
                LongMemEvalGranularity::Session.as_str().to_string(),
                session_metrics,
            );
        }

        let ranked_items = search_results
            .into_iter()
            .map(|result| {
                let corpus_item = items_by_id.get(&result.drawer.id).with_context(|| {
                    format!("missing benchmark metadata for {}", result.drawer.id)
                })?;
                Ok(LongMemEvalRankedItem {
                    corpus_id: result.drawer.id,
                    text: result.drawer.content,
                    timestamp: corpus_item.timestamp.clone(),
                    similarity: result.similarity,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(LongMemEvalBenchmarkResult {
            question_id: entry.question_id.clone(),
            question_type: entry.question_type.clone(),
            question: entry.question.clone(),
            answer: entry.answer.clone(),
            question_date: entry.question_date.clone(),
            answer_session_ids: entry.answer_session_ids.clone(),
            retrieval_results: LongMemEvalRetrievalResult {
                query: entry.question.clone(),
                corpus_size: corpus_items.len(),
                ranked_items,
                metrics,
            },
            is_abstention: entry.question_id.contains("_abs"),
            has_target_user_turn: has_target_user_turn(entry),
        })
    }
}

fn build_corpus_items(
    entry: &LongMemEvalEntry,
    granularity: LongMemEvalGranularity,
) -> Vec<CorpusItem> {
    let mut items = Vec::new();
    let mut chunk_index = 0_i64;
    for (session_id, session, timestamp) in zip_three(
        &entry.haystack_session_ids,
        &entry.haystack_sessions,
        &entry.haystack_dates,
    ) {
        match granularity {
            LongMemEvalGranularity::Session => {
                let user_turns: Vec<&LongMemEvalTurn> =
                    session.iter().filter(|turn| turn.role == "user").collect();
                let mut id = session_id.to_string();
                if session_id.contains("answer")
                    && user_turns
                        .iter()
                        .all(|turn| !turn.has_answer.unwrap_or(false))
                {
                    id = id.replace("answer", "noans");
                }
                items.push(CorpusItem {
                    id,
                    text: user_turns
                        .iter()
                        .map(|turn| turn.content.as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                    timestamp: timestamp.to_string(),
                    chunk_index,
                });
                chunk_index += 1;
            }
            LongMemEvalGranularity::Turn => {
                for (turn_index, turn) in session.iter().enumerate() {
                    if turn.role != "user" {
                        continue;
                    }
                    let mut id = format!("{session_id}_{}", turn_index + 1);
                    if session_id.contains("answer") && !turn.has_answer.unwrap_or(false) {
                        id = id.replace("answer", "noans");
                    }
                    items.push(CorpusItem {
                        id,
                        text: turn.content.clone(),
                        timestamp: timestamp.to_string(),
                        chunk_index,
                    });
                    chunk_index += 1;
                }
            }
        }
    }
    items
}

fn has_target_user_turn(entry: &LongMemEvalEntry) -> bool {
    entry
        .haystack_sessions
        .iter()
        .flatten()
        .any(|turn| turn.role == "user" && turn.has_answer.unwrap_or(false))
}

fn aggregate_results(
    results: &[LongMemEvalBenchmarkResult],
    dataset_path: &Path,
    granularity: LongMemEvalGranularity,
) -> LongMemEvalBenchmarkSummary {
    let mut skipped_abstention_questions = 0;
    let mut skipped_no_target_questions = 0;
    let mut metric_buckets: BTreeMap<String, BTreeMap<String, Vec<f64>>> = BTreeMap::new();

    for result in results {
        if result.is_abstention {
            skipped_abstention_questions += 1;
            continue;
        }
        if !result.has_target_user_turn {
            skipped_no_target_questions += 1;
            continue;
        }
        for (group, metrics) in &result.retrieval_results.metrics {
            let group_entry = metric_buckets.entry(group.clone()).or_default();
            for (metric_name, metric_value) in metrics {
                group_entry
                    .entry(metric_name.clone())
                    .or_default()
                    .push(*metric_value);
            }
        }
    }

    let metrics = metric_buckets
        .into_iter()
        .map(|(group, buckets)| {
            let averages = buckets
                .into_iter()
                .map(|(metric_name, values)| {
                    let average = if values.is_empty() {
                        0.0
                    } else {
                        values.iter().sum::<f64>() / values.len() as f64
                    };
                    (metric_name, average)
                })
                .collect();
            (group, averages)
        })
        .collect();

    LongMemEvalBenchmarkSummary {
        dataset_path: dataset_path.display().to_string(),
        granularity,
        total_questions: results.len(),
        evaluated_questions: results
            .len()
            .saturating_sub(skipped_abstention_questions + skipped_no_target_questions),
        skipped_abstention_questions,
        skipped_no_target_questions,
        metrics,
    }
}

fn evaluate_retrieval(ranked_ids: &[String], correct_docs: &[String], k: usize) -> (f64, f64, f64) {
    if correct_docs.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let recalled_docs: HashSet<&str> = ranked_ids.iter().take(k).map(String::as_str).collect();
    let recall_any = correct_docs
        .iter()
        .any(|doc| recalled_docs.contains(doc.as_str())) as u8 as f64;
    let recall_all = correct_docs
        .iter()
        .all(|doc| recalled_docs.contains(doc.as_str())) as u8 as f64;
    let ndcg_any = ndcg(ranked_ids, correct_docs, k);
    (recall_any, recall_all, ndcg_any)
}

fn evaluate_retrieval_turn_to_session(
    ranked_ids: &[String],
    correct_docs: &[String],
    k: usize,
) -> (f64, f64, f64) {
    let correct_sessions: Vec<String> = dedupe_preserving_order(
        correct_docs
            .iter()
            .map(|doc| strip_turn_id(doc))
            .collect::<Vec<_>>(),
    );
    let ranked_sessions = dedupe_preserving_order(
        ranked_ids
            .iter()
            .map(|doc| strip_turn_id(doc))
            .collect::<Vec<_>>(),
    );
    evaluate_retrieval(&ranked_sessions, &correct_sessions, k)
}

fn ndcg(ranked_ids: &[String], correct_docs: &[String], k: usize) -> f64 {
    if correct_docs.is_empty() {
        return 0.0;
    }

    let correct_docs: HashSet<&str> = correct_docs.iter().map(String::as_str).collect();
    let actual_relevances: Vec<f64> = ranked_ids
        .iter()
        .take(k)
        .map(|doc| {
            if correct_docs.contains(doc.as_str()) {
                1.0
            } else {
                0.0
            }
        })
        .collect();
    let ideal_hits = correct_docs.len().min(k);
    let ideal_relevances: Vec<f64> = (0..k)
        .map(|idx| if idx < ideal_hits { 1.0 } else { 0.0 })
        .collect();

    let ideal_dcg = dcg(&ideal_relevances);
    if ideal_dcg == 0.0 {
        return 0.0;
    }
    dcg(&actual_relevances) / ideal_dcg
}

fn dcg(relevances: &[f64]) -> f64 {
    relevances
        .iter()
        .enumerate()
        .map(|(index, relevance)| {
            if index == 0 {
                *relevance
            } else {
                *relevance / ((index + 1) as f64).log2()
            }
        })
        .sum()
}

fn strip_turn_id(document_id: &str) -> String {
    let mut parts = document_id.rsplitn(2, '_');
    let suffix = parts.next().unwrap_or_default();
    let prefix = parts.next().unwrap_or_default();
    if !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()) && !prefix.is_empty() {
        prefix.to_string()
    } else {
        document_id.to_string()
    }
}

fn dedupe_preserving_order(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            deduped.push(value);
        }
    }
    deduped
}

fn create_temp_benchmark_dir() -> anyhow::Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("steel-memory-longmemeval-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&path)
        .with_context(|| format!("failed to create benchmark temp dir {}", path.display()))?;
    Ok(path)
}

fn empty_metrics(granularity: LongMemEvalGranularity) -> BTreeMap<String, BTreeMap<String, f64>> {
    let mut metrics = BTreeMap::new();
    metrics.insert(granularity.as_str().to_string(), zero_metric_bucket());
    if granularity == LongMemEvalGranularity::Turn {
        metrics.insert(
            LongMemEvalGranularity::Session.as_str().to_string(),
            zero_metric_bucket(),
        );
    }
    metrics
}

fn zero_metric_bucket() -> BTreeMap<String, f64> {
    let mut bucket = BTreeMap::new();
    for cutoff in METRIC_CUTOFFS {
        bucket.insert(format!("recall_any@{cutoff}"), 0.0);
        bucket.insert(format!("recall_all@{cutoff}"), 0.0);
        bucket.insert(format!("ndcg_any@{cutoff}"), 0.0);
    }
    bucket
}

fn write_results_jsonl(path: &Path, results: &[LongMemEvalBenchmarkResult]) -> anyhow::Result<()> {
    let file = File::create(path)
        .with_context(|| format!("failed to create benchmark output {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    for result in results {
        serde_json::to_writer(&mut writer, result).with_context(|| {
            format!(
                "failed to serialize benchmark result for {}",
                result.question_id
            )
        })?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    Ok(())
}

struct CliOptions {
    data_path: PathBuf,
    benchmark_options: LongMemEvalBenchmarkOptions,
}

fn parse_cli_args(args: Vec<String>) -> anyhow::Result<Option<CliOptions>> {
    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage();
        return Ok(None);
    }

    let mut data_path = None;
    let mut granularity = LongMemEvalGranularity::Session;
    let mut max_questions = None;
    let mut output_path = None;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data" => data_path = Some(PathBuf::from(next_value(&mut args, "--data")?)),
            "--granularity" => {
                let value = next_value(&mut args, "--granularity")?;
                granularity = value.parse()?;
            }
            "--max-questions" => {
                let value = next_value(&mut args, "--max-questions")?;
                max_questions = Some(value.parse()?);
            }
            "--output" => output_path = Some(PathBuf::from(next_value(&mut args, "--output")?)),
            "--bench" => { /* passed automatically by `cargo bench`; ignore */ }
            other => bail!("unknown argument: {other}"),
        }
    }

    let data_path = data_path.ok_or_else(|| anyhow::anyhow!("--data is required"))?;
    Ok(Some(CliOptions {
        data_path,
        benchmark_options: LongMemEvalBenchmarkOptions {
            granularity,
            max_questions,
            output_path,
        },
    }))
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> anyhow::Result<String> {
    args.next()
        .ok_or_else(|| anyhow::anyhow!("missing value for {flag}"))
}

fn print_usage() {
    eprintln!(
        "Usage: longmemeval-benchmark --data <path> [--granularity session|turn] [--max-questions N] [--output <jsonl-path>]"
    );
}

fn zip_three<'a, A, B, C>(
    first: &'a [A],
    second: &'a [B],
    third: &'a [C],
) -> impl Iterator<Item = (&'a A, &'a B, &'a C)> {
    first
        .iter()
        .zip(second.iter())
        .zip(third.iter())
        .map(|((a, b), c)| (a, b, c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry() -> LongMemEvalEntry {
        LongMemEvalEntry {
            question_id: "q1".to_string(),
            question_type: "single-session-user".to_string(),
            question: "What dessert do I like?".to_string(),
            answer: "tiramisu".to_string(),
            question_date: "2024-01-10".to_string(),
            haystack_session_ids: vec!["sess_1".to_string(), "sess_answer".to_string()],
            haystack_dates: vec!["2024-01-01".to_string(), "2024-01-03".to_string()],
            haystack_sessions: vec![
                vec![
                    LongMemEvalTurn {
                        role: "user".to_string(),
                        content: "I went hiking yesterday.".to_string(),
                        has_answer: None,
                    },
                    LongMemEvalTurn {
                        role: "assistant".to_string(),
                        content: "That sounds fun.".to_string(),
                        has_answer: None,
                    },
                ],
                vec![
                    LongMemEvalTurn {
                        role: "user".to_string(),
                        content: "I do not like cheesecake.".to_string(),
                        has_answer: Some(false),
                    },
                    LongMemEvalTurn {
                        role: "user".to_string(),
                        content: "My favorite dessert is tiramisu.".to_string(),
                        has_answer: Some(true),
                    },
                ],
            ],
            answer_session_ids: vec!["sess_answer".to_string()],
        }
    }

    #[test]
    fn builds_session_corpus_items() {
        let items = build_corpus_items(&sample_entry(), LongMemEvalGranularity::Session);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "sess_1");
        assert_eq!(items[1].id, "sess_answer");
        assert!(items[1].text.contains("tiramisu"));
    }

    #[test]
    fn builds_turn_corpus_items_and_renames_non_answers() {
        let items = build_corpus_items(&sample_entry(), LongMemEvalGranularity::Turn);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].id, "sess_1_1");
        assert_eq!(items[1].id, "sess_noans_1");
        assert_eq!(items[2].id, "sess_answer_2");
    }

    #[test]
    fn turn_level_metrics_can_be_projected_to_session_level() {
        let ranked_ids = vec![
            "sess_answer_2".to_string(),
            "sess_answer_1".to_string(),
            "sess_1_1".to_string(),
        ];
        let correct_docs = vec!["sess_answer_2".to_string()];
        let (recall_any, recall_all, ndcg_any) =
            evaluate_retrieval_turn_to_session(&ranked_ids, &correct_docs, 1);
        assert_eq!(recall_any, 1.0);
        assert_eq!(recall_all, 1.0);
        assert!(ndcg_any > 0.99);
    }

    #[test]
    fn aggregate_results_skips_abstention_and_missing_targets() {
        let result = LongMemEvalBenchmarkResult {
            question_id: "q1".to_string(),
            question_type: "single-session-user".to_string(),
            question: "q".to_string(),
            answer: "a".to_string(),
            question_date: "2024-01-01".to_string(),
            answer_session_ids: vec!["sess_answer".to_string()],
            retrieval_results: LongMemEvalRetrievalResult {
                query: "q".to_string(),
                corpus_size: 2,
                ranked_items: Vec::new(),
                metrics: BTreeMap::from([(
                    "session".to_string(),
                    BTreeMap::from([("recall_all@5".to_string(), 1.0)]),
                )]),
            },
            is_abstention: false,
            has_target_user_turn: true,
        };
        let skipped_abstention = LongMemEvalBenchmarkResult {
            question_id: "q_abs".to_string(),
            is_abstention: true,
            ..result.clone()
        };
        let skipped_no_target = LongMemEvalBenchmarkResult {
            question_id: "q_no_target".to_string(),
            has_target_user_turn: false,
            ..result.clone()
        };

        let summary = aggregate_results(
            &[result, skipped_abstention, skipped_no_target],
            Path::new("/tmp/longmemeval.json"),
            LongMemEvalGranularity::Session,
        );

        assert_eq!(summary.total_questions, 3);
        assert_eq!(summary.evaluated_questions, 1);
        assert_eq!(summary.skipped_abstention_questions, 1);
        assert_eq!(summary.skipped_no_target_questions, 1);
        assert_eq!(summary.metrics["session"]["recall_all@5"], 1.0);
    }
}
