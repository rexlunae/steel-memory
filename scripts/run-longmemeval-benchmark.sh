#!/usr/bin/env bash

set -euo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
readonly DATA_DIR_DEFAULT="${REPO_ROOT}/.benchmarks/longmemeval"
readonly RESULTS_DIR_DEFAULT="${DATA_DIR_DEFAULT}/results"
readonly DATASET_BASE_URL="https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned/resolve/main"

usage() {
  cat <<'EOF'
Usage: run-longmemeval-benchmark.sh [options]

Downloads LongMemEval benchmark data (if needed) and runs the steel-memory benchmark.

Options:
  --dataset oracle|s|m|all  Dataset to run (default: oracle)
  --granularity session|turn
                            Benchmark granularity (default: session)
  --max-questions N         Limit the number of questions passed to the benchmark
  --data-dir PATH           Directory used for downloaded datasets
  --results-dir PATH        Directory used for benchmark output JSONL files
  --runner bench|bin        Use cargo bench or cargo run (default: bench)
  --download-only           Download datasets without running the benchmark
  --force-download          Re-download datasets even if they already exist
  -h, --help                Show this message
EOF
}

dataset_filename() {
  case "$1" in
    oracle) printf '%s\n' "longmemeval_oracle.json" ;;
    s) printf '%s\n' "longmemeval_s_cleaned.json" ;;
    m) printf '%s\n' "longmemeval_m_cleaned.json" ;;
    *)
      echo "unsupported dataset: $1" >&2
      exit 1
      ;;
  esac
}

ensure_downloader() {
  if command -v curl >/dev/null 2>&1; then
    printf '%s\n' "curl"
  elif command -v wget >/dev/null 2>&1; then
    printf '%s\n' "wget"
  else
    echo "need either curl or wget to download benchmark data" >&2
    exit 1
  fi
}

download_dataset() {
  local dataset="$1"
  local destination_dir="$2"
  local force_download="$3"
  local downloader="$4"
  local filename destination url

  filename="$(dataset_filename "${dataset}")"
  destination="${destination_dir}/${filename}"
  url="${DATASET_BASE_URL}/${filename}"

  if [[ -f "${destination}" && "${force_download}" != "true" ]]; then
    echo "Using existing dataset: ${destination}" >&2
    printf '%s\n' "${destination}"
    return 0
  fi

  mkdir -p "${destination_dir}"
  echo "Downloading ${filename}..." >&2
  if [[ "${downloader}" == "curl" ]]; then
    curl --fail --location --silent --show-error --output "${destination}" "${url}"
  else
    wget --quiet --output-document="${destination}" "${url}"
  fi

  printf '%s\n' "${destination}"
}

run_benchmark() {
  local runner="$1"
  local dataset="$2"
  local dataset_path="$3"
  local granularity="$4"
  local max_questions="${5:-}"
  local results_dir="$6"
  local output_path="${results_dir}/${dataset}-${granularity}.jsonl"
  local -a cargo_args

  mkdir -p "${results_dir}"
  if [[ "${runner}" == "bench" ]]; then
    cargo_args=(cargo bench --bench longmemeval --)
  else
    cargo_args=(cargo run --release --bin longmemeval-benchmark --)
  fi

  cargo_args+=(--data "${dataset_path}" --granularity "${granularity}" --output "${output_path}")
  if [[ -n "${max_questions}" ]]; then
    cargo_args+=(--max-questions "${max_questions}")
  fi

  echo "Running ${dataset} benchmark (${granularity})..."
  (
    cd "${REPO_ROOT}"
    "${cargo_args[@]}"
  )
  echo "Saved per-question results to ${output_path}"
}

main() {
  local dataset="oracle"
  local granularity="session"
  local max_questions=""
  local data_dir="${DATA_DIR_DEFAULT}"
  local results_dir="${RESULTS_DIR_DEFAULT}"
  local runner="bench"
  local download_only="false"
  local force_download="false"
  local downloader
  local current_dataset
  local dataset_path
  local -a datasets_to_run=()

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --dataset)
        dataset="${2:-}"
        shift 2
        ;;
      --granularity)
        granularity="${2:-}"
        shift 2
        ;;
      --max-questions)
        max_questions="${2:-}"
        shift 2
        ;;
      --data-dir)
        data_dir="${2:-}"
        shift 2
        ;;
      --results-dir)
        results_dir="${2:-}"
        shift 2
        ;;
      --runner)
        runner="${2:-}"
        shift 2
        ;;
      --download-only)
        download_only="true"
        shift
        ;;
      --force-download)
        force_download="true"
        shift
        ;;
      -h|--help)
        usage
        return 0
        ;;
      *)
        echo "unknown argument: $1" >&2
        usage >&2
        return 1
        ;;
    esac
  done

  case "${dataset}" in
    oracle|s|m) datasets_to_run=("${dataset}") ;;
    all) datasets_to_run=(oracle s m) ;;
    *)
      echo "invalid --dataset value: ${dataset}" >&2
      return 1
      ;;
  esac

  case "${granularity}" in
    session|turn) ;;
    *)
      echo "invalid --granularity value: ${granularity}" >&2
      return 1
      ;;
  esac

  case "${runner}" in
    bench|bin) ;;
    *)
      echo "invalid --runner value: ${runner}" >&2
      return 1
      ;;
  esac

  if [[ -n "${max_questions}" && ! "${max_questions}" =~ ^[0-9]+$ ]]; then
    echo "--max-questions must be an integer" >&2
    return 1
  fi

  downloader="$(ensure_downloader)"

  for current_dataset in "${datasets_to_run[@]}"; do
    dataset_path="$(download_dataset "${current_dataset}" "${data_dir}" "${force_download}" "${downloader}")"
    if [[ "${download_only}" != "true" ]]; then
      run_benchmark "${runner}" "${current_dataset}" "${dataset_path}" "${granularity}" "${max_questions}" "${results_dir}"
    fi
  done
}

main "$@"
