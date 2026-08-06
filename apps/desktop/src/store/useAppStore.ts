import { create } from "zustand";
import { desktop, type Document, type Session } from "../lib/desktop";

interface AppStore {
  session: Session | null;
  documentContent: string;
  status: string;
  error: string | null;
  selectedTab: "welcome" | "editor";
  createProject: (name: string, root: string) => Promise<void>;
  openProject: (root: string) => Promise<void>;
  saveDocument: () => Promise<void>;
  selectDocument: (document: Document) => Promise<void>;
}

export const useAppStore = create<AppStore>((set, get) => ({
  session: null,
  documentContent: "",
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
      set({ error: String(error), status: "" });
    }
  },

  async openProject(root) {
    set({ status: "打开项目...", error: null });
    try {
      const session = await desktop.openProject(root);
      const content = await desktop.openDocument(session.current_document.id);
      set({ session, documentContent: content, selectedTab: "editor", status: "项目已打开" });
    } catch (error) {
      set({ error: String(error), status: "" });
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
      set({ error: String(error), status: "" });
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
}));
