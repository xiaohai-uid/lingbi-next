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
  documentContent: string;
  candidate: GeneratedCandidate | null;
  generating: boolean;
  status: string;
  error: string | null;
  selectedTab: "welcome" | "editor";
  createProject: (name: string, root: string) => Promise<void>;
  openProject: (root: string) => Promise<void>;
  saveDocument: () => Promise<void>;
  selectDocument: (document: Document) => Promise<void>;
  generate: (instruction: string) => Promise<void>;
  adoptCandidate: () => Promise<void>;
  rejectCandidate: () => Promise<void>;
}

export const useAppStore = create<AppStore>((set, get) => ({
  session: null,
  documentContent: "",
  candidate: null,
  generating: false,
  status: "",
  error: null,
  selectedTab: "welcome",

  async createProject(name, root) {
    set({ status: "创建项目...", error: null });
    try {
      const session = await desktop.createProject(name, root);
      const content = await desktop.openDocument(session.current_document.id);
      set({
        session,
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
      set({ session, documentContent: content, selectedTab: "editor", status: "项目已打开" });
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
        status: "已保存",
      });
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
    }));
  },

  async generate(instruction) {
    const session = get().session;
    if (!session) return;
    set({ generating: true, error: null, status: "生成中..." });
    try {
      const candidate = await desktop.generate(
        session.current_document.id,
        instruction,
      );
      set({
        candidate,
        generating: false,
        status: "候选已生成",
      });
    } catch (error) {
      set({ generating: false, error: toCommandError(error).message, status: "" });
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
