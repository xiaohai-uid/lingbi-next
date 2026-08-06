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
  chapter_id: string;
  instruction: string;
  content: string;
  status: string;
  created_at: string;
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
      chapter_id: chapterId,
      instruction,
      content: "第一章正文：雨夜，林渊推开旧车站的门。",
      status: "pending",
      created_at: new Date().toISOString(),
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
