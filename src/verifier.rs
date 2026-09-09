use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub voxel: String,
    pub severity: DiagnosticSeverity,
    pub line: usize,
    pub message: String,
    pub snippet: Option<String>,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConfidenceGrade {
    High,     // >= 0.85: Firmly grounded in empirical data/equations/citations
    Moderate, // 0.65..0.85: Supported but has minor unverified assumptions
    Low,      // 0.40..0.65: Weakly grounded or missing concrete baseline comparisons
    Fragile,  // < 0.40: Unsubstantiated superlative, hallucination risk, or contradicts table/math
}

impl ConfidenceGrade {
    pub fn from_score(score: f64) -> Self {
        if score >= 0.85 {
            ConfidenceGrade::High
        } else if score >= 0.65 {
            ConfidenceGrade::Moderate
        } else if score >= 0.40 {
            ConfidenceGrade::Low
        } else {
            ConfidenceGrade::Fragile
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimReport {
    pub line: usize,
    pub passage: String,
    pub claim_type: String,
    pub confidence_score: f64,
    pub confidence_grade: ConfidenceGrade,
    pub rationale: String,
    pub evidence_found: Option<String>,
    pub suggestion: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VerificationReport {
    pub file_path: String,
    pub total_lines: usize,
    pub lyapunov_stability: f64,
    pub passed: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub summary: DiagnosticSummary,
    #[serde(default)]
    pub claims: Vec<ClaimReport>,
    #[serde(default)]
    pub mean_confidence_score: Option<f64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DiagnosticSummary {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
}

#[derive(Debug, Clone)]
pub struct CandidateClaim {
    pub line: usize,
    pub passage: String,
    pub claim_type: String,
}

pub struct DocumentVerifier;

impl DocumentVerifier {
    /// Run all synchronous cognitive voxel verification passes on a LaTeX document.
    pub fn verify<P: AsRef<Path>>(
        tex_path: P,
        bib_path: Option<P>,
    ) -> anyhow::Result<VerificationReport> {
        let tex_path = tex_path.as_ref();
        let content = fs::read_to_string(tex_path)?;
        let total_lines = content.lines().count();

        let mut diagnostics = Vec::new();

        // Voxel 1: Markdown Pollution & AST Syntax Hygiene
        diagnostics.extend(Self::voxel_hygiene(&content));

        // Voxel 2: Table & Bolding Arithmetic Verification
        diagnostics.extend(Self::voxel_table_extrema(&content));

        // Voxel 3: Citation & BibTeX Bijective Integrity
        let resolved_bib = bib_path
            .map(|p| p.as_ref().to_path_buf())
            .or_else(|| Self::infer_bib_path(tex_path, &content));

        if let Some(ref bib) = resolved_bib {
            if bib.exists() {
                if let Ok(bib_content) = fs::read_to_string(bib) {
                    diagnostics.extend(Self::voxel_bib(&content, &bib_content));
                }
            } else {
                diagnostics.push(Diagnostic {
                    voxel: "VoxelBib".to_string(),
                    severity: DiagnosticSeverity::Warning,
                    line: 0,
                    message: format!("Referenced BibTeX file '{}' does not exist.", bib.display()),
                    snippet: None,
                    suggestion: Some("Ensure the .bib file exists in the directory.".to_string()),
                });
            }
        }

        // Voxel 4: Cross-Reference & Label Bijection Integrity
        diagnostics.extend(Self::voxel_ref(tex_path, &content));

        let mut summary = DiagnosticSummary::default();
        for d in &diagnostics {
            match d.severity {
                DiagnosticSeverity::Error => summary.errors += 1,
                DiagnosticSeverity::Warning => summary.warnings += 1,
                DiagnosticSeverity::Info => summary.infos += 1,
            }
        }

        let lyapunov_stability =
            -1.0 + (0.35 * summary.errors as f64) + (0.05 * summary.warnings as f64);
        let passed = summary.errors == 0;

        Ok(VerificationReport {
            file_path: tex_path.to_string_lossy().to_string(),
            total_lines,
            lyapunov_stability,
            passed,
            diagnostics,
            summary,
            claims: Vec::new(),
            mean_confidence_score: None,
        })
    }

    /// Asynchronously run all cognitive voxels, including cross-reference verification and semantic claim confidence evaluation.
    pub async fn verify_async<P: AsRef<Path>>(
        tex_path: P,
        bib_path: Option<P>,
        check_claims: bool,
        llm: Option<&crate::llm::LlmClient>,
        max_claims: usize,
    ) -> anyhow::Result<VerificationReport> {
        let tex_path = tex_path.as_ref();
        let content = fs::read_to_string(tex_path)?;
        let total_lines = content.lines().count();

        let mut diagnostics = Vec::new();

        // Voxel 1: Markdown Pollution & AST Syntax Hygiene
        diagnostics.extend(Self::voxel_hygiene(&content));

        // Voxel 2: Table & Bolding Arithmetic Verification
        diagnostics.extend(Self::voxel_table_extrema(&content));

        // Voxel 3: Citation & BibTeX Bijective Integrity
        let resolved_bib = bib_path
            .map(|p| p.as_ref().to_path_buf())
            .or_else(|| Self::infer_bib_path(tex_path, &content));

        if let Some(ref bib) = resolved_bib {
            if bib.exists() {
                if let Ok(bib_content) = fs::read_to_string(bib) {
                    diagnostics.extend(Self::voxel_bib(&content, &bib_content));
                }
            } else {
                diagnostics.push(Diagnostic {
                    voxel: "VoxelBib".to_string(),
                    severity: DiagnosticSeverity::Warning,
                    line: 0,
                    message: format!("Referenced BibTeX file '{}' does not exist.", bib.display()),
                    snippet: None,
                    suggestion: Some("Ensure the .bib file exists in the directory.".to_string()),
                });
            }
        }

        // Voxel 4: Cross-Reference & Label Bijection Integrity
        diagnostics.extend(Self::voxel_ref(tex_path, &content));

        // Voxel 5: Semantic Claim Verification & Passage Confidence Scoring
        let mut claims = Vec::new();
        if check_claims {
            let (claim_reports, claim_diags) = Self::voxel_claim(&content, llm, max_claims).await;
            claims = claim_reports;
            diagnostics.extend(claim_diags);
        }

        let mut summary = DiagnosticSummary::default();
        for d in &diagnostics {
            match d.severity {
                DiagnosticSeverity::Error => summary.errors += 1,
                DiagnosticSeverity::Warning => summary.warnings += 1,
                DiagnosticSeverity::Info => summary.infos += 1,
            }
        }

        let fragile_count = claims
            .iter()
            .filter(|c| c.confidence_grade == ConfidenceGrade::Fragile)
            .count();
        let lyapunov_stability = -1.0
            + (0.35 * summary.errors as f64)
            + (0.05 * summary.warnings as f64)
            + (0.20 * fragile_count as f64);
        let passed = summary.errors == 0;

        let mean_confidence_score = if claims.is_empty() {
            None
        } else {
            Some(claims.iter().map(|c| c.confidence_score).sum::<f64>() / claims.len() as f64)
        };

        Ok(VerificationReport {
            file_path: tex_path.to_string_lossy().to_string(),
            total_lines,
            lyapunov_stability,
            passed,
            diagnostics,
            summary,
            claims,
            mean_confidence_score,
        })
    }

    /// Voxel 1: Hygiene - Detect raw markdown syntax that pollutes LaTeX.
    pub fn voxel_hygiene(tex_content: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let md_bold_re = Regex::new(r"\*\*([^*]+)\*\*").unwrap();
        let md_italic_re = Regex::new(r"(^|[^\\])\*([^* \t\n\r][^*]*[^* \t\n\r]|\w)\*").unwrap();
        let md_header_re = Regex::new(r"^\s*#{1,6}\s+").unwrap();

        let mut in_body = false;
        let mut in_verbatim = false;
        let has_document_env = tex_content.contains(r"\begin{document}");

        for (idx, line) in tex_content.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();

            if trimmed.starts_with("%") {
                continue;
            }

            if trimmed.contains(r"\begin{document}") {
                in_body = true;
                continue;
            }
            if trimmed.contains(r"\end{document}") {
                in_body = false;
                continue;
            }

            if has_document_env && !in_body {
                continue;
            }

            if trimmed.contains(r"\begin{verbatim}") || trimmed.contains(r"\begin{lstlisting}") {
                in_verbatim = true;
                continue;
            }
            if trimmed.contains(r"\end{verbatim}") || trimmed.contains(r"\end{lstlisting}") {
                in_verbatim = false;
                continue;
            }
            if in_verbatim {
                continue;
            }

            if md_header_re.is_match(trimmed) {
                diagnostics.push(Diagnostic {
                    voxel: "VoxelHygiene".to_string(),
                    severity: DiagnosticSeverity::Error,
                    line: line_num,
                    message: "Raw Markdown header syntax detected in LaTeX body.".to_string(),
                    snippet: Some(trimmed.to_string()),
                    suggestion: Some(
                        "Use \\section{...} or \\subsection{...} instead.".to_string(),
                    ),
                });
            }

            for caps in md_bold_re.captures_iter(line) {
                if let Some(m) = caps.get(1) {
                    diagnostics.push(Diagnostic {
                        voxel: "VoxelHygiene".to_string(),
                        severity: DiagnosticSeverity::Error,
                        line: line_num,
                        message: format!("Raw Markdown bold '**{}**' detected.", m.as_str()),
                        snippet: Some(line.to_string()),
                        suggestion: Some(format!("Replace with '\\textbf{{{}}}'.", m.as_str())),
                    });
                }
            }
            let line_sans_bold = md_bold_re.replace_all(line, " ").to_string();

            if let Some(caps) = md_italic_re.captures(&line_sans_bold) {
                if let Some(m) = caps.get(2) {
                    let match_str = m.as_str();
                    if !match_str.contains('$') && !match_str.contains('^') && !match_str.is_empty()
                    {
                        diagnostics.push(Diagnostic {
                            voxel: "VoxelHygiene".to_string(),
                            severity: DiagnosticSeverity::Error,
                            line: line_num,
                            message: format!("Raw Markdown italic '*{}*' detected (will compile as literal asterisks).", match_str),
                            snippet: Some(line.to_string()),
                            suggestion: Some(format!("Replace with '\\textit{{{}}}'.", match_str)),
                        });
                    }
                }
            }
        }

        diagnostics
    }

    /// Voxel 2: Table & Bolding Arithmetic - Checks if \textbf{} aligns with true column extrema.
    pub fn voxel_table_extrema(tex_content: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let table_re =
            Regex::new(r"(?s)\\begin\{tabular\}\s*\{([^}]+)\}(.*?)\\end\{tabular\}").unwrap();
        let num_re = Regex::new(r"[-+]?[0-9]*\.?[0-9]+").unwrap();
        let bold_re = Regex::new(r"\\textbf\{([^}]+)\}").unwrap();

        for table_cap in table_re.captures_iter(tex_content) {
            let table_body = table_cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let lines: Vec<&str> = table_body.lines().collect();

            let mut col_directions: HashMap<usize, bool> = HashMap::new();

            let mut header_row_idx = None;
            for (idx, line) in lines.iter().enumerate() {
                if line.contains(r"\downarrow")
                    || line.contains("↓")
                    || line.contains(r"\uparrow")
                    || line.contains("↑")
                {
                    header_row_idx = Some(idx);
                    let headers: Vec<&str> = line.split('&').collect();
                    for (col_idx, header) in headers.iter().enumerate() {
                        if header.contains(r"\downarrow") || header.contains("↓") {
                            col_directions.insert(col_idx, true);
                        } else if header.contains(r"\uparrow") || header.contains("↑") {
                            col_directions.insert(col_idx, false);
                        }
                    }
                    break;
                }
            }

            if col_directions.is_empty() {
                continue;
            }

            let header_idx = header_row_idx.unwrap_or(0);

            let mut col_values: HashMap<usize, Vec<(usize, f64, bool, String)>> = HashMap::new();

            for (row_idx, line) in lines.iter().enumerate() {
                if row_idx <= header_idx {
                    continue;
                }
                let trimmed = line.trim();
                if trimmed.starts_with(r"\toprule")
                    || trimmed.starts_with(r"\midrule")
                    || trimmed.starts_with(r"\bottomrule")
                    || trimmed.starts_with(r"\hline")
                {
                    continue;
                }
                if !trimmed.contains('&') {
                    continue;
                }

                let cells: Vec<&str> = trimmed.split('&').collect();
                for col_idx in col_directions.keys() {
                    if let Some(cell_text) = cells.get(*col_idx) {
                        let is_bolded = bold_re.is_match(cell_text);
                        if let Some(num_match) = num_re.find(cell_text) {
                            if let Ok(val) = num_match.as_str().parse::<f64>() {
                                col_values.entry(*col_idx).or_default().push((
                                    row_idx,
                                    val,
                                    is_bolded,
                                    cell_text.trim().to_string(),
                                ));
                            }
                        }
                    }
                }
            }

            for (col_idx, is_min) in col_directions {
                if let Some(entries) = col_values.get(&col_idx) {
                    if entries.is_empty() {
                        continue;
                    }

                    let optimum = if is_min {
                        entries.iter().map(|e| e.1).fold(f64::INFINITY, f64::min)
                    } else {
                        entries
                            .iter()
                            .map(|e| e.1)
                            .fold(f64::NEG_INFINITY, f64::max)
                    };

                    for (_row_idx, val, is_bolded, cell_snippet) in entries {
                        let is_optimal = (val - optimum).abs() < 1e-5;

                        if *is_bolded && !is_optimal {
                            let direction_str = if is_min {
                                "minimization (↓)"
                            } else {
                                "maximization (↑)"
                            };
                            diagnostics.push(Diagnostic {
                                voxel: "VoxelTable".to_string(),
                                severity: DiagnosticSeverity::Error,
                                line: 0,
                                message: format!(
                                    "Deceptive/misleading bolding in Table: value '{}' is bolded as optimal under {}, but actual optimum is '{:.4}'.",
                                    cell_snippet, direction_str, optimum
                                ),
                                snippet: Some(cell_snippet.clone()),
                                suggestion: Some(format!("Only bold the true optimum ({:.4}) or clarify why sub-optimal result is highlighted.", optimum)),
                            });
                        }
                    }
                }
            }
        }

        diagnostics
    }

    /// Voxel 3: Citation & BibTeX Bijective Integrity
    pub fn voxel_bib(tex_content: &str, bib_content: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let cite_re = Regex::new(r"\\cite[ptay]*\{([^}]+)\}").unwrap();
        let mut cited_keys: HashSet<String> = HashSet::new();

        for line in tex_content.lines() {
            if line.trim().starts_with('%') {
                continue;
            }
            for cap in cite_re.captures_iter(line) {
                if let Some(keys_match) = cap.get(1) {
                    for key in keys_match.as_str().split(',') {
                        let clean_key = key.trim().to_string();
                        if !clean_key.is_empty() {
                            cited_keys.insert(clean_key);
                        }
                    }
                }
            }
        }

        let bib_entry_re = Regex::new(r"(?i)@\w+\s*\{\s*([a-zA-Z0-9_\-:]+)").unwrap();
        let mut bib_keys: HashSet<String> = HashSet::new();

        for cap in bib_entry_re.captures_iter(bib_content) {
            if let Some(k) = cap.get(1) {
                bib_keys.insert(k.as_str().trim().to_string());
            }
        }

        for cited in &cited_keys {
            if !bib_keys.contains(cited) {
                diagnostics.push(Diagnostic {
                    voxel: "VoxelBib".to_string(),
                    severity: DiagnosticSeverity::Error,
                    line: 0,
                    message: format!(
                        "Citation key '\\cite{{{}}}' is missing from the bibliography (.bib).",
                        cited
                    ),
                    snippet: None,
                    suggestion: Some(format!(
                        "Add '@article{{{}, ...}}' to your bibliography file.",
                        cited
                    )),
                });
            }
        }

        for bib_key in &bib_keys {
            if !cited_keys.contains(bib_key) {
                diagnostics.push(Diagnostic {
                    voxel: "VoxelBib".to_string(),
                    severity: DiagnosticSeverity::Info,
                    line: 0,
                    message: format!("Bibliography entry '{}' is defined in .bib but never cited in the document.", bib_key),
                    snippet: None,
                    suggestion: Some("Consider citing or pruning this reference.".to_string()),
                });
            }
        }

        diagnostics
    }

    /// Voxel 4: Cross-Reference & Label Integrity (Multi-file \input recursion, duplicate labels, broken refs, orphaned labels)
    pub fn voxel_ref(tex_path: &Path, content: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let files = Self::collect_tex_files(tex_path, content);

        let label_re = Regex::new(r"\\label\{([^}]+)\}").unwrap();
        let ref_re = Regex::new(r"\\(?:ref|eqref|autoref|cref|Cref|pageref)\{([^}]+)\}").unwrap();

        // 1. Collect all defined labels: key -> (file_name, line_number)
        let mut defined_labels: HashMap<String, (String, usize)> = HashMap::new();

        for (path, file_content) in &files {
            let file_display = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            for (idx, line) in file_content.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with('%') {
                    continue;
                }
                for cap in label_re.captures_iter(line) {
                    if let Some(m) = cap.get(1) {
                        let key = m.as_str().trim().to_string();
                        let line_num = idx + 1;

                        if let Some((prev_file, prev_line)) = defined_labels.get(&key) {
                            diagnostics.push(Diagnostic {
                                voxel: "VoxelRef".to_string(),
                                severity: DiagnosticSeverity::Error,
                                line: line_num,
                                message: format!(
                                    "Duplicate label '\\label{{{}}}' defined in {} at line {} (previously defined in {} at line {}).",
                                    key, file_display, line_num, prev_file, prev_line
                                ),
                                snippet: Some(line.to_string()),
                                suggestion: Some(format!("Rename duplicate label '\\label{{{}}}' to avoid ambiguity.", key)),
                            });
                        } else {
                            defined_labels.insert(key, (file_display.clone(), line_num));
                        }
                    }
                }
            }
        }

        // 2. Validate references against defined labels and record cited labels
        let mut referenced_labels: HashSet<String> = HashSet::new();

        for (path, file_content) in &files {
            let file_display = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            for (idx, line) in file_content.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with('%') {
                    continue;
                }
                for cap in ref_re.captures_iter(line) {
                    if let Some(m) = cap.get(1) {
                        let ref_keys = m.as_str().split(',');
                        let line_num = idx + 1;

                        for k in ref_keys {
                            let clean_key = k.trim().to_string();
                            if clean_key.is_empty() {
                                continue;
                            }
                            referenced_labels.insert(clean_key.clone());

                            if !defined_labels.contains_key(&clean_key) {
                                diagnostics.push(Diagnostic {
                                    voxel: "VoxelRef".to_string(),
                                    severity: DiagnosticSeverity::Error,
                                    line: line_num,
                                    message: format!(
                                        "Unresolved cross-reference '\\ref{{{}}}' in {} at line {}: label not defined in document or included files.",
                                        clean_key, file_display, line_num
                                    ),
                                    snippet: Some(line.to_string()),
                                    suggestion: Some(format!("Ensure '\\label{{{}}}' is defined or check for typos.", clean_key)),
                                });
                            }
                        }
                    }
                }
            }
        }

        // 3. Detect orphaned labels (defined but never referenced)
        for (label_key, (file_display, line_num)) in &defined_labels {
            if !referenced_labels.contains(label_key) {
                diagnostics.push(Diagnostic {
                    voxel: "VoxelRef".to_string(),
                    severity: DiagnosticSeverity::Info,
                    line: *line_num,
                    message: format!(
                        "Label '\\label{{{}}}' in {} at line {} is never referenced.",
                        label_key, file_display, line_num
                    ),
                    snippet: None,
                    suggestion: Some(format!(
                        "Consider referencing '\\ref{{{}}}' or removing unused label.",
                        label_key
                    )),
                });
            }
        }

        diagnostics
    }

    /// Recursively discover and collect all .tex files included via \input{...} or \include{...}
    fn collect_tex_files(root: &Path, root_content: &str) -> Vec<(PathBuf, String)> {
        let mut files = Vec::new();
        let mut visited = HashSet::new();

        // Always include the root file
        let root_buf = root.to_path_buf();
        let mut queue = vec![(root_buf.clone(), root_content.to_string())];
        visited.insert(root_buf.clone());

        let input_re = Regex::new(r"\\(?:input|include)\{([^}]+)\}").unwrap();

        while let Some((curr_path, curr_content)) = queue.pop() {
            let parent_dir = curr_path.parent().unwrap_or_else(|| Path::new("."));

            for line in curr_content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('%') {
                    continue;
                }
                for cap in input_re.captures_iter(trimmed) {
                    if let Some(m) = cap.get(1) {
                        let rel = m.as_str().trim();
                        let candidate_path = if rel.ends_with(".tex") {
                            parent_dir.join(rel)
                        } else {
                            parent_dir.join(format!("{}.tex", rel))
                        };

                        let can_path = candidate_path
                            .canonicalize()
                            .unwrap_or_else(|_| candidate_path.clone());
                        if !visited.contains(&can_path) && candidate_path.exists() {
                            visited.insert(can_path.clone());
                            if let Ok(included_content) = fs::read_to_string(&candidate_path) {
                                queue.push((can_path, included_content));
                            }
                        }
                    }
                }
            }

            files.push((curr_path, curr_content));
        }

        files
    }

    /// Voxel 5: Semantic Claim Verification & Passage Confidence Scoring
    pub async fn voxel_claim(
        tex_content: &str,
        llm: Option<&crate::llm::LlmClient>,
        max_claims: usize,
    ) -> (Vec<ClaimReport>, Vec<Diagnostic>) {
        let candidates = Self::extract_candidate_claims(tex_content, max_claims);
        let doc_context = Self::extract_document_context(tex_content);

        let mut claims = Vec::new();
        let mut diagnostics = Vec::new();

        for candidate in candidates {
            let report = Self::evaluate_claim(&candidate, &doc_context, llm).await;

            if report.confidence_grade == ConfidenceGrade::Fragile {
                diagnostics.push(Diagnostic {
                    voxel: "VoxelClaim".to_string(),
                    severity: DiagnosticSeverity::Warning,
                    line: report.line,
                    message: format!(
                        "Fragile scientific claim (confidence: {:.0}%): {}",
                        report.confidence_score * 100.0,
                        report.rationale
                    ),
                    snippet: Some(report.passage.clone()),
                    suggestion: report.suggestion.clone(),
                });
            } else if report.confidence_grade == ConfidenceGrade::Low {
                diagnostics.push(Diagnostic {
                    voxel: "VoxelClaim".to_string(),
                    severity: DiagnosticSeverity::Info,
                    line: report.line,
                    message: format!(
                        "Low confidence claim (confidence: {:.0}%): {}",
                        report.confidence_score * 100.0,
                        report.rationale
                    ),
                    snippet: Some(report.passage.clone()),
                    suggestion: report.suggestion.clone(),
                });
            }

            claims.push(report);
        }

        (claims, diagnostics)
    }

    /// Extract high-salience candidate claims (Abstract, Contributions, Theorems, Quantitative Assertions)
    pub fn extract_candidate_claims(tex_content: &str, max_claims: usize) -> Vec<CandidateClaim> {
        let mut candidates = Vec::new();
        let mut seen_passages = HashSet::new();

        let abstract_re = Regex::new(r"(?s)\\begin\{abstract\}(.*?)\\end\{abstract\}").unwrap();

        // 1. Extract Abstract sentences
        if let Some(cap) = abstract_re.captures(tex_content) {
            if let Some(m) = cap.get(1) {
                let abs_text = m.as_str();
                for sentence in abs_text.split('.') {
                    let cleaned = sentence.trim().replace('\n', " ");
                    if cleaned.len() > 35 && seen_passages.insert(cleaned.clone()) {
                        candidates.push(CandidateClaim {
                            line: 1,
                            passage: cleaned,
                            claim_type: "Abstract / Core Thesis".to_string(),
                        });
                        if candidates.len() >= max_claims {
                            return candidates;
                        }
                    }
                }
            }
        }

        // 2. Extract Formal Theorems / Definitions
        let theorem_re = Regex::new(r"(?s)\\begin\{theorem\}(.*?)\\end\{theorem\}").unwrap();
        let lemma_re = Regex::new(r"(?s)\\begin\{lemma\}(.*?)\\end\{lemma\}").unwrap();
        let def_re = Regex::new(r"(?s)\\begin\{definition\}(.*?)\\end\{definition\}").unwrap();
        let prop_re = Regex::new(r"(?s)\\begin\{proposition\}(.*?)\\end\{proposition\}").unwrap();

        for (re, env_name) in [
            (&theorem_re, "Formal theorem"),
            (&lemma_re, "Formal lemma"),
            (&def_re, "Formal definition"),
            (&prop_re, "Formal proposition"),
        ] {
            for cap in re.captures_iter(tex_content) {
                if let Some(body) = cap.get(1) {
                    let cleaned = body.as_str().trim().replace('\n', " ");
                    if cleaned.len() > 25 && seen_passages.insert(cleaned.clone()) {
                        candidates.push(CandidateClaim {
                            line: 1,
                            passage: cleaned,
                            claim_type: env_name.to_string(),
                        });
                        if candidates.len() >= max_claims {
                            return candidates;
                        }
                    }
                }
            }
        }

        // 3. Extract quantitative or strong contribution statements line by line
        let strong_re = Regex::new(
            r"(?i)\b(outperforms?|state-of-the-art|sota|superior|accelerates?|orders of magnitude|we propose|we achieve|reduces latency by|\d+(\.\d+)?%|$\times$)\b"
        ).unwrap();

        for (idx, line) in tex_content.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();
            if trimmed.starts_with('%') || trimmed.starts_with(r"\item") && trimmed.len() < 25 {
                continue;
            }

            if strong_re.is_match(trimmed) {
                let cleaned = trimmed.replace(r"\noindent", "").trim().to_string();
                if cleaned.len() > 30 && seen_passages.insert(cleaned.clone()) {
                    let claim_type = if cleaned.to_lowercase().contains("propose")
                        || cleaned.to_lowercase().contains("introduce")
                    {
                        "Key Contribution".to_string()
                    } else {
                        "Quantitative / Empirical".to_string()
                    };

                    candidates.push(CandidateClaim {
                        line: line_num,
                        passage: cleaned,
                        claim_type,
                    });

                    if candidates.len() >= max_claims {
                        break;
                    }
                }
            }
        }

        candidates
    }

    /// Extract condensed document context: table captions & headers, equation labels, section titles
    pub fn extract_document_context(tex_content: &str) -> String {
        let mut context = String::new();

        let sec_re = Regex::new(r"\\section\{([^}]+)\}").unwrap();
        let table_re = Regex::new(r"(?s)\\begin\{table.*?\}(.*?)\\end\{table.*?\}").unwrap();
        let eq_re = Regex::new(r"(?s)\\begin\{equation\}(.*?)\\end\{equation\}").unwrap();

        context.push_str("SECTIONS:\n");
        for cap in sec_re.captures_iter(tex_content) {
            if let Some(m) = cap.get(1) {
                context.push_str(&format!("- {}\n", m.as_str().trim()));
            }
        }

        context.push_str("\nTABLES:\n");
        for (i, cap) in table_re.captures_iter(tex_content).enumerate() {
            if let Some(m) = cap.get(1) {
                let body = m.as_str().trim();
                let snippet: String = body.lines().take(6).collect::<Vec<_>>().join("\n");
                context.push_str(&format!("Table {}:\n{}\n---\n", i + 1, snippet));
            }
        }

        context.push_str("\nEQUATIONS:\n");
        for cap in eq_re.captures_iter(tex_content) {
            if let Some(m) = cap.get(1) {
                let eq_str = m.as_str().trim().replace('\n', " ");
                context.push_str(&format!("- {}\n", eq_str));
            }
        }

        context
    }

    /// Evaluate a single claim using LLM with deterministic rule-based semantic heuristic fallback
    pub async fn evaluate_claim(
        candidate: &CandidateClaim,
        doc_context: &str,
        llm: Option<&crate::llm::LlmClient>,
    ) -> ClaimReport {
        if let Some(client) = llm {
            let system_prompt = "You are a world-class scientific reviewer and formal paper verification auditor.\n\
                Evaluate the candidate claim extracted from a research paper against the provided document context (sections, tables, equations).\n\
                Assess if the claim is mathematically/empirically supported or an ungrounded hallucination/superlative.\n\
                You MUST return ONLY a JSON object with this schema:\n\
                {\n  \"confidence_score\": <float from 0.0 to 1.0>,\n  \"rationale\": \"<concise 1-2 sentence explanation>\",\n  \"evidence_found\": \"<specific table/eq or null>\",\n  \"suggestion\": \"<constructive recommendation or null>\"\n}";

            let user_prompt = format!(
                "DOCUMENT CONTEXT:\n{}\n\nCANDIDATE CLAIM (Line {}):\n\"{}\"\nCLAIM TYPE: {}\n\nEvaluate claim confidence:",
                doc_context, candidate.line, candidate.passage, candidate.claim_type
            );

            let messages = vec![
                crate::llm::Message {
                    role: "system".to_string(),
                    content: Some(system_prompt.to_string()),
                    tool_calls: None,
                    tool_call_id: None,
                },
                crate::llm::Message {
                    role: "user".to_string(),
                    content: Some(user_prompt),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ];

            if let Ok(resp) = client.complete(messages, Some(0.1)).await {
                if let Some(parsed) = Self::parse_claim_json(&resp) {
                    let score = parsed.0.clamp(0.0, 1.0);
                    let grade = ConfidenceGrade::from_score(score);
                    return ClaimReport {
                        line: candidate.line,
                        passage: candidate.passage.clone(),
                        claim_type: candidate.claim_type.clone(),
                        confidence_score: score,
                        confidence_grade: grade,
                        rationale: parsed.1,
                        evidence_found: parsed.2,
                        suggestion: parsed.3,
                    };
                }
            }
        }

        // Fallback: Deterministic rule-based semantic heuristic
        let (score, rationale, evidence, suggestion) =
            Self::heuristic_claim_evaluation(candidate, doc_context);
        let grade = ConfidenceGrade::from_score(score);

        ClaimReport {
            line: candidate.line,
            passage: candidate.passage.clone(),
            claim_type: candidate.claim_type.clone(),
            confidence_score: score,
            confidence_grade: grade,
            rationale,
            evidence_found: evidence,
            suggestion,
        }
    }

    /// Parse LLM JSON response for claim evaluation
    fn parse_claim_json(resp: &str) -> Option<(f64, String, Option<String>, Option<String>)> {
        // Strip markdown code fence if present
        let cleaned = if let Some(start) = resp.find('{') {
            if let Some(end) = resp.rfind('}') {
                &resp[start..=end]
            } else {
                resp
            }
        } else {
            resp
        };

        if let Ok(v) = serde_json::from_str::<serde_json::Value>(cleaned) {
            let score = v["confidence_score"].as_f64()?;
            let rationale = v["rationale"]
                .as_str()
                .unwrap_or("Claim evaluated.")
                .to_string();
            let evidence = v["evidence_found"].as_str().map(|s| s.to_string());
            let suggestion = v["suggestion"].as_str().map(|s| s.to_string());
            return Some((score, rationale, evidence, suggestion));
        }
        None
    }

    /// Deterministic rule-based semantic heuristic claim evaluation
    pub fn heuristic_claim_evaluation(
        candidate: &CandidateClaim,
        _doc_context: &str,
    ) -> (f64, String, Option<String>, Option<String>) {
        let lower = candidate.passage.to_lowercase();
        let has_cite = candidate.passage.contains(r"\cite");
        let has_ref = candidate.passage.contains(r"\ref") || candidate.passage.contains(r"\eqref");

        let is_superlative = lower.contains("state-of-the-art")
            || lower.contains("sota")
            || lower.contains("unprecedented")
            || lower.contains("revolutionary")
            || lower.contains("drastically")
            || lower.contains("superior");

        let has_numbers = candidate.passage.chars().any(|c| c.is_ascii_digit());

        if is_superlative && !has_cite && !has_ref {
            (
                0.35,
                "Ungrounded superlative claim: passage makes aggressive performance assertions without direct literature citation or empirical table cross-reference.".to_string(),
                None,
                Some("Cite baseline benchmarks or provide explicit \\ref{} to supporting experimental table.".to_string()),
            )
        } else if has_ref
            && (lower.contains("tab") || lower.contains("table") || lower.contains("eq"))
        {
            (
                0.90,
                "Grounded claim: passage directly cross-references internal empirical table or mathematical equation evidence.".to_string(),
                Some("Internal cross-reference (table/equation) present in claim.".to_string()),
                None,
            )
        } else if has_cite && has_numbers {
            (
                0.85,
                "Supported quantitative claim: passage cites published literature alongside specific numerical figures.".to_string(),
                Some("External citation and quantitative data verified.".to_string()),
                None,
            )
        } else if candidate.claim_type.contains("Formal") {
            (
                0.80,
                "Formal mathematical specification: statement appears within a structured theorem or definition environment.".to_string(),
                Some("Mathematical environment definition verified.".to_string()),
                None,
            )
        } else if has_cite {
            (
                0.75,
                "Supported assertion: claim is grounded with relevant external bibliographic references.".to_string(),
                Some("Citation detected.".to_string()),
                None,
            )
        } else {
            (
                0.55,
                "General assertion: passage lacks explicit quantitative references, citations, or equations.".to_string(),
                None,
                Some("Consider providing empirical verification or citations to substantiate this assertion.".to_string()),
            )
        }
    }

    /// Try to infer the .bib file path from \bibliography{...} command
    fn infer_bib_path(tex_path: &Path, content: &str) -> Option<PathBuf> {
        let bib_re = Regex::new(r"\\bibliography\{([^}]+)\}").unwrap();
        if let Some(cap) = bib_re.captures(content) {
            if let Some(m) = cap.get(1) {
                let name = m.as_str().trim();
                let file_name = if name.ends_with(".bib") {
                    name.to_string()
                } else {
                    format!("{}.bib", name)
                };

                let parent = tex_path.parent().unwrap_or_else(|| Path::new("."));
                let candidate = parent.join(&file_name);
                return Some(candidate);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voxel_hygiene_markdown_detection() {
        let bad_tex = r"
        \begin{abstract}
        *Phytophthora infestans* is a dangerous pathogen.
        **A-Block** accelerates sampling.
        ## New Section
        \end{abstract}
        ";

        let diagnostics = DocumentVerifier::voxel_hygiene(bad_tex);
        assert_eq!(diagnostics.len(), 3);
        assert!(diagnostics
            .iter()
            .any(|d| d.message.contains("Phytophthora infestans")));
        assert!(diagnostics.iter().any(|d| d.message.contains("A-Block")));
        assert!(diagnostics
            .iter()
            .any(|d| d.message.contains("Markdown header")));
    }

    #[test]
    fn test_voxel_hygiene_clean_tex() {
        let clean_tex = r"
        \begin{abstract}
        \textit{Phytophthora infestans} is a dangerous pathogen.
        \textbf{A-Block} accelerates sampling.
        \section{New Section}
        \end{abstract}
        ";

        let diagnostics = DocumentVerifier::voxel_hygiene(clean_tex);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_voxel_table_deceptive_bolding() {
        let table_with_cheat = r"
        \begin{tabular}{lcc}
        \toprule
        Method & Loss $\downarrow$ & Acc $\uparrow$ \\
        \midrule
        Baseline & 0.05 & 80.0 \\
        Proposed & \textbf{0.12} & \textbf{95.0} \\
        \bottomrule
        \end{tabular}
        ";

        let diagnostics = DocumentVerifier::voxel_table_extrema(table_with_cheat);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Deceptive/misleading bolding"));
        assert!(diagnostics[0].message.contains("0.12"));
    }

    #[test]
    fn test_voxel_table_honest_bolding() {
        let table_honest = r"
        \begin{tabular}{lcc}
        \toprule
        Method & Loss $\downarrow$ & Acc $\uparrow$ \\
        \midrule
        Baseline & \textbf{0.05} & 80.0 \\
        Proposed & 0.12 & \textbf{95.0} \\
        \bottomrule
        \end{tabular}
        ";

        let diagnostics = DocumentVerifier::voxel_table_extrema(table_honest);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_voxel_bib_matching() {
        let tex = r"
        We build on \citep{adamala2026} and \cite{chan2019}.
        Also see \cite{missing_paper}.
        ";

        let bib = r"
        @article{adamala2026,
            title={Synthetic Cells},
            author={Adamala, K},
            year={2026}
        }
        @article{chan2019,
            title={Lenia},
            author={Chan, B},
            year={2019}
        }
        @article{unused_paper,
            title={Unused},
            author={Smith, J},
            year={2020}
        }
        ";

        let diagnostics = DocumentVerifier::voxel_bib(tex, bib);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .collect();
        let infos: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Info)
            .collect();

        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("missing_paper"));

        assert_eq!(infos.len(), 1);
        assert!(infos[0].message.contains("unused_paper"));
    }

    #[test]
    fn test_voxel_ref_duplicate_and_unresolved() {
        let tex = r"
        \section{Introduction}\label{sec:intro}
        We show in Eq.~\eqref{eq:missing} and Eq.~\eqref{eq:one} our method.
        \begin{equation}\label{eq:one}
        y = x
        \end{equation}
        \begin{equation}\label{eq:one}
        z = 2x
        \end{equation}
        \begin{equation}\label{eq:orphan}
        w = 3x
        \end{equation}
        See Section~\ref{sec:intro}.
        ";

        let diagnostics = DocumentVerifier::voxel_ref(Path::new("dummy.tex"), tex);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .collect();
        let infos: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Info)
            .collect();

        // Should have 2 errors: 1 duplicate label (eq:one), 1 unresolved ref (eq:missing)
        assert_eq!(errors.len(), 2);
        assert!(errors
            .iter()
            .any(|d| d.message.contains("Duplicate label") && d.message.contains("eq:one")));
        assert!(errors
            .iter()
            .any(|d| d.message.contains("Unresolved cross-reference")
                && d.message.contains("eq:missing")));

        // Should have 1 info: orphaned label eq:orphan
        assert_eq!(infos.len(), 1);
        assert!(infos[0].message.contains("eq:orphan"));
    }

    #[test]
    fn test_voxel_ref_clean() {
        let tex = r"
        \section{Introduction}\label{sec:intro}
        As shown in Section~\ref{sec:intro} and Eq.~\eqref{eq:one}.
        \begin{equation}\label{eq:one}
        y = x
        \end{equation}
        ";

        let diagnostics = DocumentVerifier::voxel_ref(Path::new("dummy.tex"), tex);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .collect();
        assert_eq!(errors.len(), 0);
    }

    #[test]
    fn test_heuristic_claim_evaluation() {
        let superlative_cand = CandidateClaim {
            line: 10,
            passage:
                "Our model delivers unprecedented state-of-the-art results over all competitors."
                    .to_string(),
            claim_type: "Quantitative / Empirical".to_string(),
        };
        let (score, _, _, _) = DocumentVerifier::heuristic_claim_evaluation(&superlative_cand, "");
        assert!(score < 0.40);
        assert_eq!(ConfidenceGrade::from_score(score), ConfidenceGrade::Fragile);

        let grounded_cand = CandidateClaim {
            line: 20,
            passage: "As shown in Table~\\ref{tab:ostrom}, our method achieves 95% accuracy."
                .to_string(),
            claim_type: "Quantitative / Empirical".to_string(),
        };
        let (score2, _, _, _) = DocumentVerifier::heuristic_claim_evaluation(&grounded_cand, "");
        assert!(score2 >= 0.85);
        assert_eq!(ConfidenceGrade::from_score(score2), ConfidenceGrade::High);
    }
}
