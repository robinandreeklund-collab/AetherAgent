export interface SemanticNode {
  id: number;
  role: string;
  label: string;
  value?: string;
  state: NodeState;
  action?: string;
  relevance: number;
  trust: "Untrusted" | "Structural" | "Annotated";
  children: SemanticNode[];
  html_id?: string;
  name?: string;
}

export interface NodeState {
  disabled: boolean;
  checked?: boolean;
  expanded?: boolean;
  focused: boolean;
  visible: boolean;
}

export interface InjectionWarning {
  node_id: number;
  reason: string;
  severity: "Low" | "Medium" | "High";
  raw_text: string;
}

export interface SemanticTree {
  url: string;
  title: string;
  goal: string;
  nodes: SemanticNode[];
  injection_warnings: InjectionWarning[];
  parse_time_ms: number;
}

export interface TopNodesResult {
  url: string;
  title: string;
  goal: string;
  top_nodes: SemanticNode[];
  injection_warnings: number;
  parse_time_ms: number;
}

export interface ClickResult {
  found: boolean;
  node_id: number;
  role: string;
  label: string;
  action: string;
  relevance: number;
  selector_hint: string;
  trust: "Untrusted" | "Structural" | "Annotated";
  injection_warnings: InjectionWarning[];
  parse_time_ms: number;
}

export interface FormFieldMapping {
  field_label: string;
  field_role: string;
  node_id: number;
  matched_key: string;
  value: string;
  selector_hint: string;
  confidence: number;
}

export interface FillFormResult {
  mappings: FormFieldMapping[];
  unmapped_keys: string[];
  unmapped_fields: string[];
  injection_warnings: InjectionWarning[];
  parse_time_ms: number;
}

export interface ExtractedEntry {
  key: string;
  value: string;
  source_node_id: number;
  confidence: number;
}

export interface ExtractDataResult {
  entries: ExtractedEntry[];
  missing_keys: string[];
  injection_warnings: InjectionWarning[];
  parse_time_ms: number;
}

export interface WorkflowStep {
  step_index: number;
  action: string;
  url: string;
  goal: string;
  summary: string;
  timestamp_ms: number;
}

export interface WorkflowMemory {
  steps: WorkflowStep[];
  context: Record<string, string>;
}

export interface HealthResult {
  status: string;
  version: string;
  engine: string;
}

export interface InjectionCheckResult {
  safe?: boolean;
  node_id?: number;
  reason?: string;
  severity?: "Low" | "Medium" | "High";
  raw_text?: string;
}

export interface FieldChange {
  field: string;
  before: string;
  after: string;
}

export interface NodeChange {
  node_id: number;
  change_type: "Added" | "Removed" | "Modified";
  role: string;
  label: string;
  changes: FieldChange[];
}

export interface SemanticDelta {
  url: string;
  goal: string;
  total_nodes_before: number;
  total_nodes_after: number;
  changes: NodeChange[];
  token_savings_ratio: number;
  summary: string;
  diff_time_ms: number;
}

export interface JsEvalResult {
  value?: string;
  error?: string;
  timed_out: boolean;
  eval_time_us: number;
}

export interface JsBatchResult {
  results: JsEvalResult[];
  total_eval_time_us: number;
}

export interface DetectedSnippet {
  snippet_type: "InlineScript" | "EventHandler" | "TemplateExpression" | "ValueExpression";
  code: string;
  source: string;
  affects_content: boolean;
}

export interface JsDetectionResult {
  snippets: DetectedSnippet[];
  has_framework: boolean;
  framework_hint?: string;
  total_inline_scripts: number;
  total_event_handlers: number;
}

export interface JsNodeBinding {
  node_id: number;
  target_selector: string;
  target_property: string;
  expression: string;
  computed_value?: string;
  applied: boolean;
}

export interface JsAnalysisSummary {
  total_snippets: number;
  evaluable_expressions: number;
  dom_targeted_expressions: number;
  successful_bindings: number;
  failed_evaluations: number;
  frameworks_detected: string[];
}

export interface SelectiveExecResult {
  tree: SemanticTree;
  bindings: JsNodeBinding[];
  analysis: JsAnalysisSummary;
  exec_time_ms: number;
}

// ─── Fas 5: Temporal Memory types ─────────────────────────────────────────

export interface TemporalSnapshot {
  step: number;
  timestamp_ms: number;
  url: string;
  node_count: number;
  warning_count: number;
  delta?: SemanticDelta;
}

export interface NodeVolatility {
  node_id: number;
  role: string;
  label: string;
  change_count: number;
  observation_count: number;
  volatility: number;
}

export interface AdversarialPattern {
  pattern_type: "EscalatingInjection" | "GradualInjection" | "SuspiciousVolatility" | "TrustLevelShift" | "StructuralManipulation";
  description: string;
  confidence: number;
  affected_steps: number[];
  affected_node_ids: number[];
}

export interface TemporalAnalysis {
  snapshots: TemporalSnapshot[];
  node_volatility: NodeVolatility[];
  adversarial_patterns: AdversarialPattern[];
  risk_score: number;
  summary: string;
  analysis_time_ms: number;
}

export interface TemporalMemory {
  snapshots: TemporalSnapshot[];
  last_tree_json?: string;
  warning_history: Record<string, number>;
  change_history: Record<string, number>;
  observation_history: Record<string, number>;
  node_labels: Record<string, [string, string]>;
}

export interface PredictedState {
  expected_node_count: number;
  expected_warning_count: number;
  likely_changed_nodes: number[];
  confidence: number;
}

// ─── Fas 6: Intent Compiler types ─────────────────────────────────────────

export interface SubGoal {
  index: number;
  description: string;
  action_type: "Navigate" | "Click" | "Fill" | "Extract" | "Wait" | "Verify";
  depends_on: number[];
  estimated_cost: number;
  status: "Pending" | "Ready" | "InProgress" | "Completed" | "Failed";
}

export interface ActionPlan {
  goal: string;
  sub_goals: SubGoal[];
  execution_order: number[][];
  total_steps: number;
  parallel_groups: number;
  estimated_total_cost: number;
  compile_time_ms: number;
}

export interface RecommendedAction {
  sub_goal_index: number;
  action_type: "Navigate" | "Click" | "Fill" | "Extract" | "Wait" | "Verify";
  description: string;
  target_label?: string;
  fill_fields?: Record<string, string>;
  extract_keys?: string[];
  confidence: number;
}

export interface PrefetchEntry {
  expected_url: string;
  probability: number;
  precomputed_tree?: SemanticTree;
}

export interface PlanExecutionResult {
  plan: ActionPlan;
  current_step: number;
  next_action?: RecommendedAction;
  prefetch_suggestions: PrefetchEntry[];
  summary: string;
}

// ─── Fas 7: HTTP Fetch Types ──────────────────────────────────────────────

export interface FetchConfig {
  user_agent?: string;
  timeout_ms?: number;
  max_redirects?: number;
  respect_robots_txt?: boolean;
  extra_headers?: Record<string, string>;
}

export interface FetchResult {
  final_url: string;
  status_code: number;
  content_type: string;
  body: string;
  redirect_chain: string[];
  fetch_time_ms: number;
  body_size_bytes: number;
}

export interface FetchAndParseResult {
  fetch: FetchResult;
  tree: SemanticTree;
  total_time_ms: number;
}

export interface FetchAndClickResult {
  fetch: FetchResult;
  click: ClickResult;
  total_time_ms: number;
}

export interface FetchAndExtractResult {
  fetch: FetchResult;
  extract: ExtractDataResult;
  total_time_ms: number;
}

export interface FetchAndPlanResult {
  fetch: FetchResult;
  plan_json: string;
  execution_json: string;
  total_time_ms: number;
}

// ─── WASM Agent (local, no network) ──────────────────────────────────────

export declare class AetherAgent {
  constructor();
  health(): HealthResult;
  parse(html: string, goal: string, url: string): SemanticTree;
  parseTop(html: string, goal: string, url: string, topN?: number): TopNodesResult;
  findAndClick(html: string, goal: string, url: string, targetLabel: string): ClickResult;
  fillForm(html: string, goal: string, url: string, fields: Record<string, string>): FillFormResult;
  extractData(html: string, goal: string, url: string, keys: string[]): ExtractDataResult;
  diffTrees(oldTreeJson: string | SemanticTree, newTreeJson: string | SemanticTree): SemanticDelta;
  detectJs(html: string): JsDetectionResult;
  evalJs(code: string): JsEvalResult;
  evalJsBatch(snippets: string[]): JsBatchResult;
  parseWithJs(html: string, goal: string, url: string): SelectiveExecResult;
  checkInjection(text: string): InjectionCheckResult;
  wrapUntrusted(content: string): string;
  createTemporalMemory(): TemporalMemory;
  addTemporalSnapshot(memoryJson: string | TemporalMemory, html: string, goal: string, url: string, timestampMs: number): TemporalMemory;
  analyzeTemporal(memoryJson: string | TemporalMemory): TemporalAnalysis;
  predictTemporal(memoryJson: string | TemporalMemory): PredictedState;
  compileGoal(goal: string): ActionPlan;
  executePlan(planJson: string | ActionPlan, html: string, goal: string, url: string, completedSteps?: number[]): PlanExecutionResult;
  createMemory(): WorkflowMemory;
  addStep(memoryJson: string | WorkflowMemory, action: string, url: string, goal: string, summary: string): WorkflowMemory;
  setContext(memoryJson: string | WorkflowMemory, key: string, value: string): WorkflowMemory;
  getContext(memoryJson: string | WorkflowMemory, key: string): { value: string | null };
}

// ─── HTTP Client (connects to deployed server, supports fetch) ─────────

export declare class AetherAgentHTTP {
  constructor(baseUrl?: string);
  health(): Promise<HealthResult>;
  parse(html: string, goal: string, url: string): Promise<SemanticTree>;
  findAndClick(html: string, goal: string, url: string, targetLabel: string): Promise<ClickResult>;
  compileGoal(goal: string): Promise<ActionPlan>;
  executePlan(planJson: string | ActionPlan, html: string, goal: string, url: string, completedSteps?: number[]): Promise<PlanExecutionResult>;
  fetchRaw(url: string, config?: FetchConfig): Promise<FetchResult>;
  fetchParse(url: string, goal: string, config?: FetchConfig): Promise<FetchAndParseResult>;
  fetchClick(url: string, goal: string, targetLabel: string, config?: FetchConfig): Promise<FetchAndClickResult>;
  fetchExtract(url: string, goal: string, keys: string[], config?: FetchConfig): Promise<FetchAndExtractResult>;
  fetchPlan(url: string, goal: string, completedSteps?: number[], config?: FetchConfig): Promise<FetchAndPlanResult>;
}
