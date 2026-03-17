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
  checkInjection(text: string): InjectionCheckResult;
  wrapUntrusted(content: string): string;
  createMemory(): WorkflowMemory;
  addStep(memoryJson: string | WorkflowMemory, action: string, url: string, goal: string, summary: string): WorkflowMemory;
  setContext(memoryJson: string | WorkflowMemory, key: string, value: string): WorkflowMemory;
  getContext(memoryJson: string | WorkflowMemory, key: string): { value: string | null };
}
