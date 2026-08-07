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
    return [...mockSessions.entries()].map(([root, session]) => ({
      name: session.project.name,
      root,
      last_opened: new Date().toISOString(),
    }));
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
  ): Promise<void> {
    if (inTauri()) {
      await invoke("provider_configure", {
        providerId,
        key,
        baseUrl,
        model,
      });
    }
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
    return {
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
    return [];
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
    const session = [...mockSessions.values()].at(-1);
    if (!session) throw new Error("no session");
    return {
      ...session.current_document,
      revision: expectedRevision + 1,
      updated_at: new Date().toISOString(),
    };
  },

  async candidateReject(candidateId: string): Promise<void> {
    if (inTauri()) {
      await invoke("candidate_reject", { candidateId });
    }
  },
};
