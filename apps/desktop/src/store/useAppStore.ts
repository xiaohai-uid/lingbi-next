import { create } from "zustand";
import {
  desktop,
  toCommandError,
  type Document,
  type GeneratedCandidate,
  type Session,
} from "../lib/desktop";

interface AppStore {
  session: Session | null;
  documents: Document[];
  documentContent: string;
  candidate: GeneratedCandidate | null;
  generating: boolean;
  generationTaskId: string | null;
  status: string;
  error: string | null;
  selectedTab: "welcome" | "editor";
  createProject: (name: string, root: string) => Promise<void>;
  openProject: (root: string) => Promise<void>;
  saveDocument: () => Promise<void>;
  createChapter: () => Promise<void>;
  selectDocument: (document: Document) => Promise<void>;
  generate: (instruction: string) => Promise<void>;
  cancelGeneration: () => Promise<void>;
  adoptCandidate: () => Promise<void>;
  rejectCandidate: () => Promise<void>;
}

export const useAppStore = create<AppStore>((set, get) => ({
  session: null,
  documents: [],
  documentContent: "",
  candidate: null,
  generating: false,
  generationTaskId: null,
  status: "",
  error: null,
  selectedTab: "welcome",

  async createProject(name, root) {
    set({ status: "创建项目...", error: null });
    try {
      const session = await desktop.createProject(name, root);
      const content = await desktop.openDocument(session.current_document.id);
      const documents = await desktop.listDocuments();
      set({
        session,
        documents,
        documentContent: content,
        selectedTab: "editor",
        status: "项目已创建",
      });
    } catch (error) {
      set({ error: toCommandError(error).message, status: "" });
    }
  },

  async openProject(root) {
    set({ status: "打开项目...", error: null });
    try {
      const session = await desktop.openProject(root);
      const content = await desktop.openDocument(session.current_document.id);
      const documents = await desktop.listDocuments();
      set({ session, documents, documentContent: content, selectedTab: "editor", status: "项目已打开" });
    } catch (error) {
      set({ error: toCommandError(error).message, status: "" });
    }
  },

  async saveDocument() {
    const session = get().session;
    if (!session) return;
    set({ status: "保存中...", error: null });
    try {
      const document = await desktop.saveDocument(
        session.current_document.id,
        session.current_document.revision,
        get().documentContent,
      );
      set({
        session: {
          ...session,
          current_document: document,
          dirty: false,
        },
        documents: get().documents.map((item) =>
          item.id === document.id ? document : item,
        ),
        status: "已保存",
      });
    } catch (error) {
      set({ error: toCommandError(error).message, status: "" });
    }
  },

  async createChapter() {
    const session = get().session;
    if (!session) return;
    set({ status: "创建章节...", error: null });
    try {
      const documents = await desktop.listDocuments();
      const nextTitle =
        documents.length === 1 ? "第二章" : `第${documents.length + 1}章`;
      const document = await desktop.createDocument(
        session.project.id,
        nextTitle,
        "",
      );
      const updatedDocuments = [...documents, document];
      const content = await desktop.openDocument(document.id);
      set((state) => ({
        session: state.session
          ? { ...state.session, current_document: document, dirty: false }
          : null,
        documents: updatedDocuments,
        documentContent: content,
        status: "章节已创建",
      }));
    } catch (error) {
      set({ error: toCommandError(error).message, status: "" });
    }
  },

  async selectDocument(document) {
    const content = await desktop.openDocument(document.id);
    set((state) => ({
      session: state.session
        ? { ...state.session, current_document: document }
        : null,
      documentContent: content,
      status: "已切换",
    }));
  },

  async generate(instruction) {
    const session = get().session;
    if (!session) return;
    set({ generating: true, error: null, status: "生成中...", generationTaskId: null });
    try {
      const candidate = await desktop.generate(
        session.current_document.id,
        instruction,
        (taskId) => useAppStore.setState({ generationTaskId: taskId }),
      );
      set({
        candidate,
        generating: false,
        generationTaskId: null,
        status: "候选已生成",
      });
    } catch (error) {
      set({
        generating: false,
        generationTaskId: null,
        error: toCommandError(error).message,
        status: "",
      });
    }
  },

  async cancelGeneration() {
    const taskId = get().generationTaskId;
    if (!taskId) return;
    try {
      await desktop.generationCancel(taskId);
      set({ generating: false, generationTaskId: null, status: "已取消" });
    } catch (error) {
      set({ error: toCommandError(error).message, status: "" });
    }
  },

  async adoptCandidate() {
    const session = get().session;
    const candidate = get().candidate;
    if (!session || !candidate) return;
    set({ status: "采纳中...", error: null });
    try {
      const document = await desktop.candidateAdopt(
        candidate.id,
        session.current_document.revision,
      );
      set({
        session: {
          ...session,
          current_document: document,
          dirty: false,
        },
        documents: get().documents.map((item) =>
          item.id === document.id ? document : item,
        ),
        documentContent: candidate.content,
        candidate: null,
        status: "已采纳",
      });
    } catch (error) {
      set({ error: toCommandError(error).message, status: "" });
    }
  },

  async rejectCandidate() {
    const candidate = get().candidate;
    if (!candidate) return;
    try {
      await desktop.candidateReject(candidate.id);
      set({ candidate: null, status: "已拒绝" });
    } catch (error) {
      set({ error: toCommandError(error).message, status: "" });
    }
  },
}));
