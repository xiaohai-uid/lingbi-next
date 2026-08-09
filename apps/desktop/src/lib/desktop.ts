import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface Project {
  id: string;
  name: string;
  schema_version: number;
  created_at: string;
  updated_at: string;
}

export interface Document {
  id: string;
  project_id: string;
  title: string;
  order: number;
  revision: number;
  content_hash: string;
  created_at: string;
  updated_at: string;
}

export interface Session {
  project: Project;
  current_document: Document;
  dirty: boolean;
  root: string;
  recovered?: boolean;
  protected?: boolean;
}

export interface GeneratedCandidate {
  id: string;
  project_id: string;
  document_id: string;
  instruction: string;
  base_revision: number;
  base_content_hash: string;
  content: string;
  content_hash: string;
  provider_id: string;
  model_id: string;
  status: string;
  created_at: string;
  approved_at: string | null;
  committed_at: string | null;
}

export interface RecentProject {
  name: string;
  root: string;
  last_opened: string;
  /// "v2" = V2 应用模型；"v3" = Book V3 书仓（P5-1 起）；旧记录缺省 "v2"。
  kind?: string;
}

export interface ProviderDefinition {
  id: string;
  display_name: string;
  protocol: string;
  default_endpoint: string;
  recommended_model: string;
  models: string[];
}

export interface ProviderTestResult {
  provider_id: string;
  model_id: string;
  ok: boolean;
  latency_ms: number;
  error: string | null;
}

/// P5-5：provider_status 回显（含持久化的高级设置，永不含 API key）。
export interface ProviderStatus {
  configured: boolean;
  provider_id: string;
  model_id: string;
  temperature: number;
  max_tokens: number;
}

/// P5-5 声明式技巧卡（只读展示，无执行能力）。
export interface SkillCard {
  id: string;
  title: string;
  description: string;
  steps: string[];
}

export interface ExportResult {
  format: string;
  path: string;
}

export interface CommandError {
  code: string;
  message: string;
  retryable: boolean;
}

// ---- Book V3（P4 命令层 / P5-1 前端接线）----

export interface BookSession {
  name: string;
  root: string;
  managed: boolean;
  state_revision: number;
  chapter_count: number;
  recovered: number;
  recovery_failed: number;
  recovery_errors: string[];
}

export interface Chapter {
  ch: number;
  title: string;
  frontmatter: Record<string, string>;
  body: string;
  content_hash: string;
}

export interface ChapterMeta {
  ch: number;
  title: string;
}

export interface TxReceipt {
  tx_id: string;
  rel_path: string;
  after_revision: number | null;
  after_content_hash: string;
}

/// P9 #26 章事务链节点（start_chain/get_chain_status 返回）。
export interface ChainNode {
  node_id: string;
  definition_id: string;
  definition_version: number;
  input_hash: string;
  output_hash: string;
  status: string;
  provider: string | null;
  model: string | null;
  usage: { prompt_tokens: number; completion_tokens: number } | null;
  error: string | null;
  note: string | null;
}

/// P9 #26 交付报告（handoff 节点摘要）。
export interface HandoffInfo {
  ch: number;
  chapter_path: string;
  title: string;
  words: number;
  nodes: Array<{ definition_id: string; status: string }>;
}

/// P9 #26 章事务链运行结果。
export interface ChainRun {
  ch: number;
  status: string;
  run_id: string;
  repair_attempts: number;
  reject_reason: string | null;
  book_input_hash: string;
  nodes: ChainNode[];
  checks: Array<{ ok: boolean; name: string; detail: string }>;
  draft_path: string | null;
  review_path: string | null;
  handoff: HandoffInfo | null;
}

/// P9 #26 运行状态（轮询）。
export interface ChainStatus {
  run_id: string;
  done: boolean;
  status: string | null;
  current_node: string | null;
  repair_attempts: number | null;
  result: ChainRun | null;
  error: string | null;
}

export interface GenerationEvent {
  type: "delta" | "candidate" | "error" | "cancelled";
  task_id: string;
  content?: string;
  candidate?: GeneratedCandidate;
  error?: CommandError;
}

/// P5-2 Book V3 生成事件（book-generation-event）。
export interface BookGenerationEvent {
  type: "delta" | "candidate" | "error" | "cancelled";
  task_id: string;
  content?: string;
  candidate_id?: string;
  ch?: number;
  error?: CommandError;
}

export interface BookCandidate {
  candidate_id: string;
  ch: number;
  content: string;
}

export function toCommandError(error: unknown): CommandError {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error &&
    "retryable" in error
  ) {
    const candidate = error as Record<string, unknown>;
    return {
      code: String(candidate.code),
      message: String(candidate.message),
      retryable: Boolean(candidate.retryable),
    };
  }
  return {
    code: "UNKNOWN",
    message: String(error),
    retryable: false,
  };
}

const inTauri = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const mockSessions = new Map<string, Session>();
const mockDocuments = new Map<string, Document[]>();
const mockContents = new Map<string, string>();
const mockCandidates = new Map<string, GeneratedCandidate[]>();
// Book V3 mock（vitest 非 Tauri 环境）：root → 书仓内存态
const mockBooks = new Map<
  string,
  {
    name: string;
    chapters: Map<number, { title: string; content: string; hash: string }>;
  }
>();

function mockHash(content: string): string {
  // 确定性伪哈希（非加密；mock 层只用于 CAS 相等性比较）
  let value = 5381;
  for (const char of content) {
    value = (value * 33 + char.charCodeAt(0)) >>> 0;
  }
  return value.toString(16).padStart(64, "0");
}
export let lastProviderConfig: {
  providerId: string;
  key: string;
  baseUrl?: string;
  model?: string;
  temperature?: number;
  maxTokens?: number;
} | null = null;

async function generateStreaming(
  chapterId: string,
  instruction: string,
  onStart?: (taskId: string) => void,
  onDelta?: (content: string) => void,
): Promise<GeneratedCandidate> {
  return new Promise<GeneratedCandidate>((resolve, reject) => {
    let unlisten: (() => void) | undefined;
    let taskId: string | null = null;
    const cleanup = () => {
      unlisten?.();
    };
    listen<GenerationEvent>("generation-event", (event) => {
      if (taskId === null) {
        taskId = event.payload.task_id;
      } else if (event.payload.task_id !== taskId) {
        return;
      }
      if (event.payload.type === "delta" && event.payload.content) {
        onDelta?.(event.payload.content);
      } else if (event.payload.type === "candidate" && event.payload.candidate) {
        cleanup();
        resolve(event.payload.candidate);
      } else if (event.payload.type === "error" && event.payload.error) {
        cleanup();
        reject(event.payload.error);
      } else if (event.payload.type === "cancelled") {
        cleanup();
        reject(
          toCommandError({
            code: "AI_CANCELLED",
            message: "AI generation cancelled",
            retryable: false,
          }),
        );
      }
    })
      .then(async (unlistenFn) => {
        unlisten = unlistenFn;
        const started = await invoke<{ task_id: string }>("generation_start", {
          chapterId,
          instruction,
        });
        taskId = started.task_id;
        onStart?.(taskId);
      })
      .catch((error) => {
        cleanup();
        reject(error);
      });
  });
}

export const desktop = {
  async createProject(name: string, root?: string): Promise<Session> {
    if (inTauri()) {
      return invoke<Session>("project_create", { name, root });
    }
    const resolvedRoot = root || name;
    const now = new Date().toISOString();
    const session: Session = {
      root: resolvedRoot,
      dirty: false,
      recovered: false,
      protected: false,
      project: {
        id: crypto.randomUUID(),
        name,
        schema_version: 2,
        created_at: now,
        updated_at: now,
      },
      current_document: {
        id: crypto.randomUUID(),
        project_id: crypto.randomUUID(),
        title: "第一章",
        order: 0,
        revision: 0,
        content_hash: "",
        created_at: now,
        updated_at: now,
      },
    };
    mockSessions.set(resolvedRoot, session);
    mockDocuments.set(resolvedRoot, [session.current_document]);
    mockContents.set(session.current_document.id, "# 第一章\n\n浏览器预览模式");
    return session;
  },

  async openProject(root: string): Promise<Session> {
    if (inTauri()) {
      return invoke<Session>("project_open", { root });
    }
    const session = mockSessions.get(root);
    if (!session) throw new Error("project not found");
    return session;
  },

  async getSession(): Promise<Session | null> {
    if (inTauri()) {
      return invoke<Session | null>("project_get_session");
    }
    return [...mockSessions.values()].at(-1) ?? null;
  },

  async projectDefaultRoot(name: string): Promise<string> {
    if (inTauri()) {
      return invoke<string>("project_default_root", { name });
    }
    return `文档/LingBi/${name}`;
  },

  async recentProjects(): Promise<RecentProject[]> {
    if (inTauri()) {
      return invoke<RecentProject[]>("recent_projects");
    }
    return [
      ...[...mockSessions.entries()].map(([root, session]) => ({
        name: session.project.name,
        root,
        last_opened: new Date().toISOString(),
        kind: "v2",
      })),
      ...[...mockBooks.entries()].map(([root, book]) => ({
        name: book.name,
        root,
        last_opened: new Date().toISOString(),
        kind: "v3",
      })),
    ];
  },

  async listDocuments(): Promise<Document[]> {
    if (inTauri()) {
      return invoke<Document[]>("document_list");
    }
    const session = [...mockSessions.values()].at(-1);
    if (!session) return [];
    return mockDocuments.get(session.root) ?? [];
  },

  async createDocument(
    projectId: string,
    title: string,
    content: string,
  ): Promise<Document> {
    if (inTauri()) {
      return invoke<Document>("document_create", {
        projectId,
        title,
        content,
      });
    }
    const session = [...mockSessions.values()].at(-1);
    if (!session) throw new Error("no session");
    const now = new Date().toISOString();
    const document: Document = {
      id: crypto.randomUUID(),
      project_id: projectId,
      title,
      order: mockDocuments.get(session.root)?.length ?? 0,
      revision: 0,
      content_hash: "",
      created_at: now,
      updated_at: now,
    };
    const documents = [...(mockDocuments.get(session.root) ?? [])];
    documents.push(document);
    mockDocuments.set(session.root, documents);
    mockContents.set(document.id, content);
    return document;
  },

  async openDocument(documentId: string): Promise<string> {
    if (inTauri()) {
      return invoke<string>("document_open", { documentId });
    }
    return mockContents.get(documentId) ?? "";
  },

  async saveDocument(
    documentId: string,
    expectedRevision: number,
    content: string,
  ): Promise<Document> {
    if (inTauri()) {
      return invoke<Document>("document_save", {
        documentId,
        expectedRevision,
        content,
      });
    }
    const session = [...mockSessions.values()].at(-1);
    if (!session) throw new Error("no session");
    return {
      ...session.current_document,
      revision: expectedRevision + 1,
      updated_at: new Date().toISOString(),
    };
  },

  async exportDocument(format: string): Promise<ExportResult> {
    if (inTauri()) {
      return invoke<ExportResult>("document_export", { format });
    }
    return { format, path: `export/正文.${format}` };
  },

  async providerList(): Promise<ProviderDefinition[]> {
    if (inTauri()) {
      return invoke<ProviderDefinition[]>("provider_list");
    }
    return [
      {
        id: "openai",
        display_name: "OpenAI",
        protocol: "openai-compatible",
        default_endpoint: "https://api.openai.com/v1/chat/completions",
        recommended_model: "gpt-4o-mini",
        models: ["gpt-4o-mini", "gpt-4o"],
      },
      {
        id: "claude",
        display_name: "Claude",
        protocol: "anthropic",
        default_endpoint: "https://api.anthropic.com/v1/messages",
        recommended_model: "claude-3-5-haiku-latest",
        models: ["claude-3-5-haiku-latest", "claude-3-5-sonnet-latest"],
      },
      {
        id: "deepseek",
        display_name: "DeepSeek",
        protocol: "openai-compatible",
        default_endpoint: "https://api.deepseek.com/v1/chat/completions",
        recommended_model: "deepseek-chat",
        models: ["deepseek-chat", "deepseek-reasoner"],
      },
    ];
  },

  async providerConfigure(
    providerId: string,
    key: string,
    baseUrl?: string,
    model?: string,
    temperature?: number,
    maxTokens?: number,
  ): Promise<void> {
    if (inTauri()) {
      await invoke("provider_configure", {
        providerId,
        key,
        baseUrl,
        model,
        temperature,
        maxTokens,
      });
      return;
    }
    lastProviderConfig = {
      providerId,
      key,
      baseUrl,
      model,
      temperature,
      maxTokens,
    };
  },

  async providerStatus(): Promise<ProviderStatus> {
    if (inTauri()) {
      return invoke<ProviderStatus>("provider_status");
    }
    const cfg = lastProviderConfig;
    return {
      configured: Boolean(cfg?.key),
      provider_id: cfg?.providerId ?? "openai",
      model_id: cfg?.model ?? "",
      temperature: cfg?.temperature ?? 0.7,
      max_tokens: cfg?.maxTokens ?? 2048,
    };
  },

  /// P5-5 声明式技巧卡：只读静态数据。
  async listSkills(): Promise<SkillCard[]> {
    if (inTauri()) {
      return invoke<SkillCard[]>("list_skills");
    }
    return [
      {
        id: "theme-guide-lite",
        title: "题材引导（精简版）",
        description: "从一句话设定开始，逐步收拢题材方向与卖点。",
        steps: [
          "一句话设定：谁、在什么世界、要做什么",
          "核心卖点：读者为什么点进来",
          "冲突引擎：每章至少一个悬念或转折",
        ],
      },
      {
        id: "chapter-skeleton",
        title: "章节骨架",
        description: "一章一目标：开场、推进、钩子三段式。",
        steps: [
          "开场钩子：前 200 字抓住读者",
          "目标推进：本章信息/成长/冲突至少前进一步",
          "结尾悬念：留下一个想读下去的钩子",
        ],
      },
    ];
  },

  async testConnection(): Promise<ProviderTestResult> {
    if (inTauri()) {
      return invoke<ProviderTestResult>("provider_test");
    }
    return {
      provider_id: "openai",
      model_id: "gpt-4o-mini",
      ok: true,
      latency_ms: 120,
      error: null,
    };
  },

  async generate(
    chapterId: string,
    instruction: string,
    onStart?: (taskId: string) => void,
    onDelta?: (content: string) => void,
  ): Promise<GeneratedCandidate> {
    if (inTauri()) {
      return generateStreaming(chapterId, instruction, onStart, onDelta);
    }
    onStart?.("browser-mock-task");
    const candidate: GeneratedCandidate = {
      id: crypto.randomUUID(),
      project_id: crypto.randomUUID(),
      document_id: chapterId,
      instruction,
      base_revision: 0,
      base_content_hash: "",
      content: "第一章正文：雨夜，林渊推开旧车站的门。",
      content_hash: "",
      provider_id: "fake",
      model_id: "fake-provider",
      status: "pending",
      created_at: new Date().toISOString(),
      approved_at: null,
      committed_at: null,
    };
    mockCandidates.set(chapterId, [
      ...(mockCandidates.get(chapterId) ?? []),
      candidate,
    ]);
    return candidate;
  },

  async generationCancel(taskId: string): Promise<void> {
    if (inTauri()) {
      await invoke("generation_cancel", { taskId });
    }
  },

  async candidateList(chapterId: string): Promise<GeneratedCandidate[]> {
    if (inTauri()) {
      return invoke<GeneratedCandidate[]>("candidate_list", { chapterId });
    }
    return mockCandidates.get(chapterId) ?? [];
  },

  async candidateAdopt(
    candidateId: string,
    expectedRevision: number,
  ): Promise<Document> {
    if (inTauri()) {
      return invoke<Document>("candidate_adopt", {
        candidateId,
        expectedRevision,
      });
    }
    const candidates = [...mockCandidates.values()].flat();
    const candidate = candidates.find((item) => item.id === candidateId);
    return {
      id: "d1",
      project_id: "p1",
      title: "第一章",
      order: 0,
      revision: (candidate?.base_revision ?? expectedRevision) + 1,
      content_hash: "",
      created_at: "",
      updated_at: "",
    };
  },

  async candidateReject(candidateId: string): Promise<void> {
    if (inTauri()) {
      await invoke("candidate_reject", { candidateId });
    }
  },

  // ---- Book V3（P5-1）----

  async createBook(root: string, name: string): Promise<BookSession> {
    if (inTauri()) {
      return invoke<BookSession>("create_book", { root, name });
    }
    mockBooks.set(root, { name, chapters: new Map() });
    return {
      name,
      root,
      managed: true,
      state_revision: 1,
      chapter_count: 0,
      recovered: 0,
      recovery_failed: 0,
      recovery_errors: [],
    };
  },

  async openBook(root: string): Promise<BookSession> {
    if (inTauri()) {
      return invoke<BookSession>("open_book", { root });
    }
    const book = mockBooks.get(root);
    if (!book) throw new Error(`book not found: ${root}`);
    return {
      name: book.name,
      root,
      managed: true,
      state_revision: 1,
      chapter_count: book.chapters.size,
      recovered: 0,
      recovery_failed: 0,
      recovery_errors: [],
    };
  },

  async bookGenerate(
    ch: number,
    instruction: string,
    onStart?: (taskId: string) => void,
    onDelta?: (content: string) => void,
  ): Promise<BookCandidate> {
    if (inTauri()) {
      return new Promise<BookCandidate>((resolve, reject) => {
        let unlisten: (() => void) | undefined;
        let taskId: string | null = null;
        const cleanup = () => {
          unlisten?.();
        };
        listen<BookGenerationEvent>("book-generation-event", (event) => {
          if (taskId === null) {
            taskId = event.payload.task_id;
          } else if (event.payload.task_id !== taskId) {
            return;
          }
          if (event.payload.type === "delta" && event.payload.content) {
            onDelta?.(event.payload.content);
          } else if (
            event.payload.type === "candidate" &&
            event.payload.candidate_id
          ) {
            cleanup();
            resolve({
              candidate_id: event.payload.candidate_id,
              ch: event.payload.ch ?? ch,
              content: event.payload.content ?? "",
            });
          } else if (event.payload.type === "error" && event.payload.error) {
            cleanup();
            reject(event.payload.error);
          } else if (event.payload.type === "cancelled") {
            cleanup();
            reject(
              toCommandError({
                code: "AiCancelled",
                message: "AI generation cancelled",
                retryable: false,
              }),
            );
          }
        })
          .then(async (unlistenFn) => {
            unlisten = unlistenFn;
            const started = await invoke<{ task_id: string }>("book_generate", {
              ch,
              instruction,
            });
            taskId = started.task_id;
            onStart?.(taskId);
          })
          .catch((error) => {
            cleanup();
            reject(error);
          });
      });
    }
    // mock：异步流式（delta + onStart），与真实事件管道同形
    return new Promise<BookCandidate>((resolve) => {
      const taskId = crypto.randomUUID();
      onStart?.(taskId);
      const content = "第一章正文：雨夜，林渊推开旧车站的门。";
      // 两段 delta 模拟流式
      window.setTimeout(() => {
        onDelta?.(content.slice(0, 10));
        window.setTimeout(() => {
          onDelta?.(content.slice(10));
          resolve({
            candidate_id: `${String(ch).padStart(3, "0")}-${crypto.randomUUID()}`,
            ch,
            content,
          });
        }, 30);
      }, 30);
    });
  },

  async bookGenerateCancel(taskId: string): Promise<void> {
    if (inTauri()) {
      await invoke("book_generate_cancel", { taskId });
    }
  },

  async bookAdopt(candidateId: string): Promise<{ exit: number; stdout: string }> {
    if (inTauri()) {
      return invoke("adopt_candidate", { candidateId });
    }
    const book = [...mockBooks.values()].at(-1);
    if (!book) throw new Error("no book session");
    // mock：候选内容写入第一章（简化：ch 前缀解析）
    const ch = Number(candidateId.split("-")[0]);
    const entry = book.chapters.get(ch);
    if (!entry) throw new Error(`chapter ${ch} not found`);
    // mock：采纳内容与 bookGenerate mock 的候选一致
    const content = "第一章正文：雨夜，林渊推开旧车站的门。";
    entry.content = content;
    entry.hash = mockHash(content);
    return { exit: 0, stdout: "✅ commit" };
  },

  async bookReject(candidateId: string): Promise<void> {
    if (inTauri()) {
      await invoke("reject_candidate", { candidateId });
    }
  },

  // ---- P5-3 写作工具（只读展示面，全部经命令层）----

  async getOutline(): Promise<Array<Record<string, unknown>>> {
    if (inTauri()) {
      return invoke("get_outline");
    }
    return [];
  },

  async buildContract(ch: number): Promise<string> {
    if (inTauri()) {
      return invoke<string>("build_contract", { ch });
    }
    return '{"goal":"(无细纲,依 大纲/总纲.md 一句话)","hard_constraints":[],"forbidden":[],"foreshadow_todo":[],"ending_requirement":"","sources":[]}';
  },

  async buildBrief(ch: number): Promise<string> {
    if (inTauri()) {
      return invoke<string>("build_brief", { ch });
    }
    return "## 任务书（mock）\n\n第一段写作委托。";
  },

  async runBookCheck(stage?: string): Promise<Array<{ ok: boolean; name: string; detail: string }>> {
    if (inTauri()) {
      return invoke("run_check", { stage });
    }
    return Array.from({ length: 11 }, (_, i) => ({
      ok: true,
      name: `机检${i + 1}`,
      detail: "mock 通过",
    }));
  },

  async resumeBookWorkflow(): Promise<string> {
    if (inTauri()) {
      return invoke<string>("resume_workflow");
    }
    return '{"status":"进行中","next_step":"继续写","files":[]}';
  },

  async bookExport(
    ch: number,
    format: string,
  ): Promise<{ format: string; path: string }> {
    if (inTauri()) {
      return invoke("export_chapter", { ch, format });
    }
    return {
      format,
      path: `export/${String(ch).padStart(3, "0")}-mock.${format}`,
    };
  },

  async listChapters(): Promise<ChapterMeta[]> {
    if (inTauri()) {
      return invoke<ChapterMeta[]>("list_chapters");
    }
    const book = [...mockBooks.values()].at(-1);
    if (!book) return [];
    return [...book.chapters.entries()]
      .map(([ch, entry]) => ({ ch, title: entry.title }))
      .sort((a, b) => a.ch - b.ch);
  },

  async createChapter(title: string): Promise<Chapter> {
    if (inTauri()) {
      return invoke<Chapter>("create_chapter", { title });
    }
    const book = [...mockBooks.values()].at(-1);
    if (!book) throw new Error("no book session");
    const ch = book.chapters.size + 1;
    const entry = { title, content: "", hash: mockHash("") };
    book.chapters.set(ch, entry);
    return {
      ch,
      title,
      frontmatter: { title, chapter: String(ch).padStart(3, "0") },
      body: "",
      content_hash: entry.hash,
    };
  },

  async openChapter(ch: number): Promise<Chapter> {
    if (inTauri()) {
      return invoke<Chapter>("open_chapter", { ch });
    }
    const book = [...mockBooks.values()].at(-1);
    if (!book) throw new Error("no book session");
    const entry = book.chapters.get(ch);
    if (!entry) throw new Error(`chapter ${ch} not found`);
    return {
      ch,
      title: entry.title,
      frontmatter: { title: entry.title, chapter: String(ch).padStart(3, "0") },
      body: entry.content,
      content_hash: entry.hash,
    };
  },

  async saveChapter(
    ch: number,
    content: string,
    expectedHash?: string,
  ): Promise<TxReceipt> {
    if (inTauri()) {
      return invoke<TxReceipt>("save_chapter", { ch, content, expectedHash });
    }
    const book = [...mockBooks.values()].at(-1);
    if (!book) throw new Error("no book session");
    const entry = book.chapters.get(ch);
    if (!entry) throw new Error(`chapter ${ch} not found`);
    // CAS：期望哈希与当前不符 → DocumentConflict（与真实命令层同语义）
    if (expectedHash && expectedHash !== entry.hash) {
      throw {
        code: "DocumentConflict",
        message: "外部修改了正文,存盘被拒(用户字节保留);请重新 open_chapter 后再存",
        retryable: false,
      };
    }
    entry.content = content;
    entry.hash = mockHash(content);
    return {
      tx_id: crypto.randomUUID(),
      rel_path: `定稿/正文/${String(ch).padStart(3, "0")}-${entry.title}.md`,
      after_revision: null,
      after_content_hash: entry.hash,
    };
  },

  // ---- P9 #26 章事务链（start_chain/cancel_chain/get_chain_status）----

  async startChain(ch: number): Promise<{ run_id: string }> {
    if (inTauri()) {
      return invoke<{ run_id: string }>("start_chain", { ch });
    }
    return { run_id: `mock-${crypto.randomUUID()}` };
  },

  async cancelChain(runId: string): Promise<void> {
    if (inTauri()) {
      await invoke("cancel_chain", { runId });
    }
  },

  async getChainStatus(runId: string): Promise<ChainStatus> {
    if (inTauri()) {
      return invoke<ChainStatus>("get_chain_status", { runId });
    }
    // mock 分支：测试注入（__setMockChainStatus）或立即完成
    if (mockChainStatus && mockChainStatus.run_id === runId) {
      return mockChainStatus;
    }
    return { run_id: runId, done: true, status: "Completed", current_node: null, repair_attempts: 0, result: null, error: null };
  },
};

/// mock 分支可注入的链状态（App.test.tsx 用；生产 Tauri 路径不经过）。
let mockChainStatus: ChainStatus | null = null;
export function __setMockChainStatus(status: ChainStatus | null) {
  mockChainStatus = status;
}
