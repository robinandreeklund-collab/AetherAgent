/// AetherAgent WPT Test Runner
///
/// Kör Web Platform Tests direkt mot AetherAgents DOM-implementation.
/// Använder QuickJS sandbox + DOM bridge för att evaluera testharness.js-tester.
///
/// Användning:
///   cargo run --bin aether-wpt --features js-eval -- [WPT_DIR] [FILTER...]
///
/// Exempel:
///   cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/
///   cargo run --bin aether-wpt --features js-eval -- wpt-suite/ --filter getElementById
use std::path::{Path, PathBuf};
use std::time::Instant;

use aether_agent::arena_dom_sink;
use aether_agent::dom_bridge;

// ─── Resultattyper ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct WptTestResult {
    file: String,
    total: usize,
    passed: usize,
    failed: usize,
    timedout: usize,
    notrun: usize,
    error: Option<String>,
    cases: Vec<WptCase>,
    duration_ms: f64,
}

#[derive(Debug, Clone)]
struct WptCase {
    name: String,
    status: String,
    message: Option<String>,
}

#[derive(Debug, Default)]
struct WptSummary {
    total_files: usize,
    total_cases: usize,
    total_passed: usize,
    total_failed: usize,
    total_timedout: usize,
    total_notrun: usize,
    total_errors: usize,
    duration_ms: f64,
}

// ─── Testharness JS-filer (inbäddade) ──────────────────────────────────────

const POLYFILLS: &str = include_str!("polyfills.js");
const TESTHARNESS_SHIM: &str = include_str!("testharness-shim.js");
const TESTHARNESSREPORT: &str = include_str!("testharnessreport.js");

// ─── HTML-parsning: extrahera <script>-block ────────────────────────────────

// ─── Kör ett enskilt WPT-test ───────────────────────────────────────────────

fn run_wpt_test(html_path: &Path) -> WptTestResult {
    let file_name = html_path
        .strip_prefix(".")
        .unwrap_or(html_path)
        .display()
        .to_string();
    let start = Instant::now();

    // Läs HTML
    let html = match std::fs::read_to_string(html_path) {
        Ok(h) => h,
        Err(e) => {
            return WptTestResult {
                file: file_name,
                total: 0,
                passed: 0,
                failed: 0,
                timedout: 0,
                notrun: 0,
                error: Some(format!("Failed to read file: {}", e)),
                cases: vec![],
                duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            };
        }
    };

    // Extrahera inline scripts (skippas <script src="...">)
    // OBS: vi disablade script/style text-filtering i arena_dom_sink
    // så vi måste parsa utan den optimeringen för WPT.
    // Enklaste lösningen: bygg en separat ArenaDom och hämta scripts manuellt.
    let test_scripts = extract_scripts_for_wpt(&html);

    if test_scripts.is_empty() {
        return WptTestResult {
            file: file_name,
            total: 0,
            passed: 0,
            failed: 0,
            timedout: 0,
            notrun: 0,
            error: Some("No inline <script> blocks found".to_string()),
            cases: vec![],
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        };
    }

    // Bygg script-sekvens: polyfills → testharness → testharnessreport → testets scripts
    let mut all_scripts: Vec<String> = Vec::new();
    all_scripts.push(POLYFILLS.to_string());
    all_scripts.push(TESTHARNESS_SHIM.to_string());
    all_scripts.push(TESTHARNESSREPORT.to_string());
    all_scripts.extend(test_scripts);
    // Trigga completion om det inte redan hänt
    all_scripts.push("if (!report.complete) { done(); }".to_string());
    // Extrahera resultat
    all_scripts.push("JSON.stringify({complete: report.complete, status: report.status, log: report.log, passed: report.passed, failed: report.failed, timedout: report.timedout, notrun: report.notrun})".to_string());

    // Parsa HTML till ArenaDom (detta ger testet en DOM att jobba med)
    let arena = arena_dom_sink::parse_html_to_arena(&html);

    // Kör alla scripts med DOM bridge + lifecycle
    let result = dom_bridge::eval_js_with_lifecycle(&all_scripts, arena);

    // Parsa report-JSON från sista evalueringen
    let duration = start.elapsed().as_secs_f64() * 1000.0;

    parse_wpt_result(&file_name, &result.value, &result.error, duration)
}

/// Extrahera scripts från HTML utan whitespace/script-text-filtering
fn extract_scripts_for_wpt(html: &str) -> Vec<String> {
    // Enkel regex-fri approach: hitta <script>...</script> block
    // (vi kan inte använda arena_dom_sink pga script text filtering)
    let mut scripts = Vec::new();
    let lower = html.to_lowercase();
    let bytes = html.as_bytes();
    let lower_bytes = lower.as_bytes();

    let mut pos = 0;
    while pos < bytes.len() {
        // Hitta <script utan src
        if let Some(tag_start) = find_bytes(&lower_bytes[pos..], b"<script") {
            let abs_start = pos + tag_start;

            // Hitta slutet av öppningstaggen
            if let Some(tag_end_rel) = find_bytes(&bytes[abs_start..], b">") {
                let tag_end = abs_start + tag_end_rel + 1;

                // Kolla om taggen har src=
                let tag_content = &html[abs_start..tag_end];
                if tag_content.to_lowercase().contains("src=") {
                    pos = tag_end;
                    continue;
                }

                // Hitta </script>
                if let Some(close_rel) = find_bytes(&lower_bytes[tag_end..], b"</script>") {
                    let close_abs = tag_end + close_rel;
                    let script_text = &html[tag_end..close_abs];
                    if !script_text.trim().is_empty() {
                        scripts.push(script_text.to_string());
                    }
                    pos = close_abs + 9; // "</script>".len()
                } else {
                    pos = tag_end;
                }
            } else {
                pos = abs_start + 7;
            }
        } else {
            break;
        }
    }

    scripts
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle))
}

fn parse_wpt_result(
    file: &str,
    value: &Option<String>,
    error: &Option<String>,
    duration_ms: f64,
) -> WptTestResult {
    // Försök parsa JSON-resultat
    if let Some(ref json_str) = value {
        if let Ok(parsed) = parse_report_json(json_str) {
            return parsed.with_file(file, duration_ms);
        }
    }

    // Om vi inte fick JSON, rapportera som error
    WptTestResult {
        file: file.to_string(),
        total: 0,
        passed: 0,
        failed: 0,
        timedout: 0,
        notrun: 0,
        error: error
            .clone()
            .or_else(|| Some("No report JSON returned".to_string())),
        cases: vec![],
        duration_ms,
    }
}

struct ParsedReport {
    passed: usize,
    failed: usize,
    timedout: usize,
    notrun: usize,
    cases: Vec<WptCase>,
    status: String,
}

impl ParsedReport {
    fn with_file(self, file: &str, duration_ms: f64) -> WptTestResult {
        let total = self.passed + self.failed + self.timedout + self.notrun;
        WptTestResult {
            file: file.to_string(),
            total,
            passed: self.passed,
            failed: self.failed,
            timedout: self.timedout,
            notrun: self.notrun,
            error: if self.status == "ERROR" {
                Some("Test suite error".to_string())
            } else {
                None
            },
            cases: self.cases,
            duration_ms,
        }
    }
}

fn parse_report_json(json: &str) -> Result<ParsedReport, String> {
    // Minimal JSON-parsning utan extern crate
    // Format: {"complete":true,"status":"OK","log":"...", "passed":N, ...}
    let passed = extract_json_num(json, "passed").unwrap_or(0);
    let failed = extract_json_num(json, "failed").unwrap_or(0);
    let timedout = extract_json_num(json, "timedout").unwrap_or(0);
    let notrun = extract_json_num(json, "notrun").unwrap_or(0);
    let status = extract_json_str(json, "status").unwrap_or_default();
    let log = extract_json_str(json, "log").unwrap_or_default();

    // Parsa log-rader: "test_name|status|message\n..."
    let cases: Vec<WptCase> = log
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(3, '|').collect();
            WptCase {
                name: parts.first().unwrap_or(&"?").to_string(),
                status: parts.get(1).unwrap_or(&"?").to_string(),
                message: parts.get(2).map(|s| s.to_string()),
            }
        })
        .collect();

    Ok(ParsedReport {
        passed,
        failed,
        timedout,
        notrun,
        cases,
        status,
    })
}

fn extract_json_num(json: &str, key: &str) -> Option<usize> {
    let search = format!("\"{}\":", key);
    let idx = json.find(&search)?;
    let after = &json[idx + search.len()..];
    let trimmed = after.trim_start();
    let num_end = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    trimmed[..num_end].parse().ok()
}

fn extract_json_str(json: &str, key: &str) -> Option<String> {
    let search = format!("\"{}\":\"", key);
    let idx = json.find(&search)?;
    let after = &json[idx + search.len()..];
    // Hitta stängande " (hantera escaped quotes)
    let mut end = 0;
    let bytes = after.as_bytes();
    while end < bytes.len() {
        if bytes[end] == b'"' && (end == 0 || bytes[end - 1] != b'\\') {
            break;
        }
        end += 1;
    }
    Some(after[..end].replace("\\n", "\n").replace("\\\"", "\""))
}

// ─── Samla testfiler ────────────────────────────────────────────────────────

fn collect_test_files(dir: &Path, filter: &Option<String>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if dir.is_file() {
        files.push(dir.to_path_buf());
        return files;
    }

    let walker = walkdir(dir);
    for entry in walker {
        let path = entry;
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str());
        if ext != Some("html") && ext != Some("htm") {
            continue;
        }
        // Skippa hjälpfiler
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "testharness.js" {
            continue;
        }
        // Filtrera
        if let Some(ref f) = filter {
            let path_str = path.display().to_string();
            if !path_str.contains(f.as_str()) {
                continue;
            }
        }
        files.push(path);
    }

    files.sort();
    files
}

fn walkdir(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                result.extend(walkdir(&path));
            } else {
                result.push(path);
            }
        }
    }
    result
}

// ─── Main ───────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("AetherAgent WPT Runner");
        eprintln!();
        eprintln!("Usage: aether-wpt <WPT_DIR_OR_FILE> [--filter PATTERN] [--json] [--verbose]");
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  aether-wpt wpt-suite/dom/nodes/");
        eprintln!("  aether-wpt wpt-suite/dom/nodes/Document-getElementById.html");
        eprintln!("  aether-wpt wpt-suite/ --filter querySelector --verbose");
        eprintln!();
        eprintln!("The WPT_DIR should contain real, unmodified WPT test HTML files.");
        eprintln!("Download from: https://github.com/niccokunzmann/niccokunzmann.github.io/");
        std::process::exit(1);
    }

    let test_path = PathBuf::from(&args[1]);
    let mut filter: Option<String> = None;
    let mut json_output = false;
    let mut verbose = false;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--filter" => {
                i += 1;
                if i < args.len() {
                    filter = Some(args[i].clone());
                }
            }
            "--json" => json_output = true,
            "--verbose" => verbose = true,
            _ => {
                // Behandla som extra filter
                filter = Some(args[i].clone());
            }
        }
        i += 1;
    }

    if !test_path.exists() {
        eprintln!("Error: Path '{}' does not exist.", test_path.display());
        eprintln!();
        eprintln!("To get WPT tests, run:");
        eprintln!("  git clone --depth 1 https://github.com/niccokunzmann/niccokunzmann.github.io/ wpt-suite");
        eprintln!("  # or for lightpanda's fork:");
        eprintln!(
            "  git clone --depth 1 -b fork https://github.com/lightpanda-io/wpt.git wpt-suite"
        );
        std::process::exit(1);
    }

    let files = collect_test_files(&test_path, &filter);

    if files.is_empty() {
        eprintln!("No test files found in '{}'", test_path.display());
        if let Some(ref f) = filter {
            eprintln!("  (filter: '{}')", f);
        }
        std::process::exit(1);
    }

    eprintln!("AetherAgent WPT Runner — {} test file(s)", files.len());
    eprintln!("{}", "=".repeat(70));

    let mut summary = WptSummary::default();

    for file in &files {
        let result = run_wpt_test(file);

        summary.total_files += 1;
        summary.total_cases += result.total;
        summary.total_passed += result.passed;
        summary.total_failed += result.failed;
        summary.total_timedout += result.timedout;
        summary.total_notrun += result.notrun;
        summary.duration_ms += result.duration_ms;
        if result.error.is_some() {
            summary.total_errors += 1;
        }

        if json_output {
            // JSON output per fil
            print_json_result(&result);
        } else {
            // Human-readable output
            let status_icon = if result.error.is_some() {
                "ERR"
            } else if result.failed == 0 && result.passed > 0 {
                "OK "
            } else if result.passed > 0 {
                "MIX"
            } else {
                "---"
            };

            eprintln!(
                "[{}] {}: {} passed, {} failed, {} not run ({:.0}ms)",
                status_icon,
                result.file,
                result.passed,
                result.failed,
                result.notrun,
                result.duration_ms
            );

            if verbose {
                for case in &result.cases {
                    let icon = match case.status.as_str() {
                        "Pass" => " +",
                        "Fail" => " -",
                        "Not Run" => " ?",
                        _ => " !",
                    };
                    eprint!("  {icon} {}", case.name);
                    if let Some(ref msg) = case.message {
                        if !msg.is_empty() {
                            eprint!(": {}", msg);
                        }
                    }
                    eprintln!();
                }
                if let Some(ref err) = result.error {
                    eprintln!("  ERROR: {}", err);
                }
            }
        }
    }

    // Sammanfattning
    eprintln!();
    eprintln!("{}", "=".repeat(70));
    let pass_rate = if summary.total_cases > 0 {
        summary.total_passed as f64 / summary.total_cases as f64 * 100.0
    } else {
        0.0
    };

    eprintln!("WPT Summary:");
    eprintln!("  Files:    {}", summary.total_files);
    eprintln!("  Cases:    {}", summary.total_cases);
    eprintln!("  Passed:   {} ({:.1}%)", summary.total_passed, pass_rate);
    eprintln!("  Failed:   {}", summary.total_failed);
    eprintln!("  Timeout:  {}", summary.total_timedout);
    eprintln!("  Not Run:  {}", summary.total_notrun);
    eprintln!("  Errors:   {}", summary.total_errors);
    eprintln!("  Duration: {:.0}ms", summary.duration_ms);

    // JSON summary till stdout
    if json_output {
        println!(
            "{{\"files\":{},\"cases\":{},\"passed\":{},\"failed\":{},\"timedout\":{},\"notrun\":{},\"errors\":{},\"pass_rate\":{:.1},\"duration_ms\":{:.0}}}",
            summary.total_files,
            summary.total_cases,
            summary.total_passed,
            summary.total_failed,
            summary.total_timedout,
            summary.total_notrun,
            summary.total_errors,
            pass_rate,
            summary.duration_ms
        );
    }

    // Exit code: 0 om inga failures, 1 om det finns
    if summary.total_failed > 0 || summary.total_errors > 0 {
        std::process::exit(1);
    }
}

fn print_json_result(result: &WptTestResult) {
    let cases_json: Vec<String> = result
        .cases
        .iter()
        .map(|c| {
            format!(
                "{{\"name\":\"{}\",\"status\":\"{}\",\"message\":{}}}",
                c.name.replace('\"', "\\\""),
                c.status,
                match &c.message {
                    Some(m) => format!("\"{}\"", m.replace('\"', "\\\"")),
                    None => "null".to_string(),
                }
            )
        })
        .collect();

    println!(
        "{{\"file\":\"{}\",\"passed\":{},\"failed\":{},\"cases\":[{}]}}",
        result.file.replace('\"', "\\\""),
        result.passed,
        result.failed,
        cases_json.join(","),
    );
}
