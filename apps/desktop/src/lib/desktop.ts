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

export interface GenerationEvent {
  type: "delta" | "candidate" | "error" | "cancelled";
  task_id: string;
  content?: string;
  candidate?: GeneratedCandidate;
  error?: CommandError;
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
};
