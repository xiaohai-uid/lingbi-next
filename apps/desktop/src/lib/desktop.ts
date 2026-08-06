import { invoke } from "@tauri-apps/api/core";

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

export interface CommandError {
  code: string;
  message: string;
  retryable: boolean;
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

export const desktop = {
  async createProject(name: string, root: string): Promise<Session> {
    if (inTauri()) {
      return invoke<Session>("project_create", { name, root });
    }
    const now = new Date().toISOString();
    const session: Session = {
      root,
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
    mockSessions.set(root, session);
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
    throw new Error("document creation is not wired in browser mock");
  },

  async openDocument(documentId: string): Promise<string> {
    if (inTauri()) {
      return invoke<string>("document_open", { documentId });
    }
    return "# 第一章\n\n浏览器预览模式";
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

  async providerConfigure(
    key: string,
    baseUrl: string,
    model: string,
  ): Promise<void> {
    if (inTauri()) {
      await invoke("provider_configure", {
        key,
        baseUrl,
        model,
      });
    }
  },

  async generate(
    chapterId: string,
    instruction: string,
  ): Promise<GeneratedCandidate> {
    if (inTauri()) {
      return invoke<GeneratedCandidate>("generation_start", {
        chapterId,
        instruction,
      });
    }
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
