import { create } from "zustand";
import {
  desktop,
  toCommandError,
  type BookSession,
  type Chapter,
  type Document,
  type ExportResult,
  type GeneratedCandidate,
  type ProviderDefinition,
  type ProviderTestResult,
  type RecentProject,
  type Session,
} from "../lib/desktop";

/// Book V3 保存状态（P5-1：saved/saving/conflict 可见）。
export type SaveState = "idle" | "saving" | "saved" | "conflict";

interface AppStore {
  session: Session | null;
  documents: Document[];
  documentContent: string;
  candidate: GeneratedCandidate | null;
  generating: boolean;
  generationTaskId: string | null;
  status: string;
  error: string | null;
  errorCode: string | null;
  selectedTab: "welcome" | "editor";
  recentProjects: RecentProject[];
  providers: ProviderDefinition[];
  providerTest: ProviderTestResult | null;
  aiConfigured: boolean;
  lastExport: ExportResult | null;
  streamingText: string;
  recoveryDismissed: boolean;
  lastSavedContent: string;
  recoveryCandidate: GeneratedCandidate | null;
  // Book V3（P5-1：项目/编辑/保存/重启）
  book: BookSession | null;
  bookChapters: Chapter[];
  currentCh: number | null;
  chapterContent: string;
  chapterHash: string;
  lastSavedChapterContent: string;
  saveState: SaveState;
  bookOpen: (root: string) => Promise<void>;
  bookCreate: (root: string, name: string) => Promise<void>;
  chapterCreate: (title: string) => Promise<void>;
  chapterOpen: (ch: number) => Promise<void>;
  chapterSave: () => Promise<void>;
  dismissRecovery: () => void;
  loadRecoveryCandidate: () => Promise<void>;
  createProject: (name: string, root?: string) => Promise<void>;
  openProject: (root: string) => Promise<void>;
  loadRecent: () => Promise<void>;
  loadProviders: () => Promise<void>;
  saveDocument: () => Promise<void>;
  createChapter: () => Promise<void>;
  selectDocument: (document: Document) => Promise<void>;
  generate: (instruction: string) => Promise<void>;
  cancelGeneration: () => Promise<void>;
  adoptCandidate: () => Promise<void>;
  rejectCandidate: () => Promise<void>;
  providerConfigure: (
    providerId: string,
    key: string,
    baseUrl?: string,
    model?: string,
    temperature?: number,
    maxTokens?: number,
  ) => Promise<void>;
  testProvider: () => Promise<void>;
  exportDocument: (format: string) => Promise<void>;
}

function setError(set: (partial: Partial<AppStore>) => void, error: unknown) {
  const parsed = toCommandError(error);
  set({ error: parsed.message, errorCode: parsed.code });
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
  errorCode: null,
  selectedTab: "welcome",
  recentProjects: [],
  providers: [],
  providerTest: null,
  aiConfigured: false,
  lastExport: null,
  streamingText: "",
  recoveryDismissed: false,
  lastSavedContent: "",
  recoveryCandidate: null,
  book: null,
  bookChapters: [],
  currentCh: null,
  chapterContent: "",
  chapterHash: "",
  lastSavedChapterContent: "",
  saveState: "idle",

  // ---- Book V3（P5-1）----

  async bookCreate(root, name) {
    set({ status: "创建书仓...", error: null });
    try {
      const book = await desktop.createBook(root, name);
      const meta = await desktop.listChapters();
      const bookChapters: Chapter[] = meta.map((entry) => ({
        ch: entry.ch,
        title: entry.title,
        frontmatter: { title: entry.title },
        body: "",
        content_hash: "",
      }));
      set({
        session: null,
        book,
        bookChapters,
        currentCh: null,
        chapterContent: "",
        chapterHash: "",
        lastSavedChapterContent: "",
        saveState: "idle",
        selectedTab: "editor",
        status: "书仓已创建",
      });
      void get().loadRecent();
    } catch (error) {
      setError(set, error);
    }
  },

  async bookOpen(root) {
    set({ status: "打开书仓...", error: null });
    try {
      const book = await desktop.openBook(root);
      const meta = await desktop.listChapters();
      const bookChapters: Chapter[] = meta.map((entry) => ({
        ch: entry.ch,
        title: entry.title,
        frontmatter: { title: entry.title },
        body: "",
        content_hash: "",
      }));
      set({
        session: null,
        book,
        bookChapters,
        currentCh: null,
        chapterContent: "",
        chapterHash: "",
        lastSavedChapterContent: "",
        saveState: "idle",
        selectedTab: "editor",
        status:
          book.recovered > 0 || book.recovery_failed > 0
            ? `书仓已打开（恢复 ${book.recovered} 条，异常 ${book.recovery_failed} 条）`
            : "书仓已打开",
      });
      void get().loadRecent();
    } catch (error) {
      setError(set, error);
    }
  },

  async chapterCreate(title) {
    if (!get().book) return;
    // 防抖窗口内的未保存编辑先落盘（S2-4：建章不丢字）
    const pending = get();
    if (
      pending.currentCh !== null &&
      pending.chapterContent !== pending.lastSavedChapterContent
    ) {
      await get().chapterSave();
    }
    set({ status: "创建章节...", error: null });
    try {
      const chapter = await desktop.createChapter(title);
      const bookChapters = [...get().bookChapters, chapter];
      set({
        bookChapters,
        currentCh: chapter.ch,
        chapterContent: chapter.body,
        chapterHash: chapter.content_hash,
        lastSavedChapterContent: chapter.body,
        saveState: "saved",
        status: "章节已创建",
      });
    } catch (error) {
      setError(set, error);
    }
  },

  async chapterOpen(ch) {
    if (!get().book) return;
    // 防抖窗口内的未保存编辑先落盘（S2-4：切章不丢字）
    const pending = get();
    if (
      pending.currentCh !== null &&
      pending.currentCh !== ch &&
      pending.chapterContent !== pending.lastSavedChapterContent
    ) {
      await get().chapterSave();
    }
    set({ status: "打开章节...", error: null });
    try {
      const chapter = await desktop.openChapter(ch);
      set({
        currentCh: chapter.ch,
        chapterContent: chapter.body,
        chapterHash: chapter.content_hash,
        lastSavedChapterContent: chapter.body,
        saveState: "saved",
        status: "章节已打开",
      });
    } catch (error) {
      setError(set, error);
    }
  },

  async chapterSave() {
    const book = get().book;
    const ch = get().currentCh;
    if (!book || ch === null) return;
    // 保存内容快照：保存期间用户继续输入的增量留给下一轮防抖
    // （避免保存完成后 lastSaved 误标为最新内容导致静默丢字）。
    const content = get().chapterContent;
    const hash = get().chapterHash;
    set({ saveState: "saving", error: null, status: "正在保存…" });
    try {
      const receipt = await desktop.saveChapter(ch, content, hash || undefined);
      set({
        saveState: "saved",
        chapterHash: receipt.after_content_hash,
        lastSavedChapterContent: content,
        status: "已保存",
      });
    } catch (error) {
      setError(set, error);
      // 仅外部修改（CAS 冲突）置 conflict；其余失败保留错误面板
      // 与「重试」指引，不引导重新打开章节（那会丢弃未保存内容）。
      set({
        saveState: get().errorCode === "DocumentConflict" ? "conflict" : "saved",
        status: "保存失败",
      });
    }
  },

  dismissRecovery() {
    set({ recoveryDismissed: true });
  },

  async loadRecoveryCandidate() {
    const session = get().session;
    if (!session) return;
    try {
      const candidates = await desktop.candidateList(
        session.current_document.id,
      );
      set({ recoveryCandidate: candidates.at(-1) ?? null });
    } catch {
      set({ recoveryCandidate: null });
    }
  },

  async createProject(name, root) {
    set({ status: "创建项目...", error: null });
    try {
      const session = await desktop.createProject(name, root);
      const content = await desktop.openDocument(session.current_document.id);
      const documents = await desktop.listDocuments();
      set({
        book: null,
        session,
        documents,
        documentContent: content,
        selectedTab: "editor",
        status: "项目已创建",
        recoveryDismissed: false,
        lastSavedContent: content,
      });
      void get().loadRecent();
    } catch (error) {
      setError(set, error);
    }
  },

  async openProject(root) {
    set({ status: "打开项目...", error: null });
    try {
      const session = await desktop.openProject(root);
      const content = await desktop.openDocument(session.current_document.id);
      const documents = await desktop.listDocuments();
      set({
        book: null,
        session,
        documents,
        documentContent: content,
        selectedTab: "editor",
        status: "项目已打开",
        recoveryDismissed: false,
        lastSavedContent: content,
      });
      void get().loadRecent();
    } catch (error) {
      setError(set, error);
    }
  },

  async loadRecent() {
    try {
      const recentProjects = await desktop.recentProjects();
      set({ recentProjects });
    } catch {
      set({ recentProjects: [] });
    }
  },

  async loadProviders() {
    try {
      const providers = await desktop.providerList();
      set({ providers });
    } catch {
      set({ providers: [] });
    }
  },

  async providerConfigure(providerId, key, baseUrl, model, temperature, maxTokens) {
    set({ status: "保存 AI 设置...", error: null });
    try {
      await desktop.providerConfigure(
        providerId,
        key,
        baseUrl,
        model,
        temperature,
        maxTokens,
      );
      set({ aiConfigured: true, status: "AI 设置已保存" });
    } catch (error) {
      setError(set, error);
    }
  },

  async testProvider() {
    set({ providerTest: null, error: null, status: "测试连接..." });
    try {
      const providerTest = await desktop.testConnection();
      set({
        providerTest,
        status: providerTest.ok ? "连接成功" : "连接失败",
      });
    } catch (error) {
      set({
        providerTest: {
          provider_id: "",
          model_id: "",
          ok: false,
          latency_ms: 0,
          error: toCommandError(error).message,
        },
        status: "连接失败",
      });
    }
  },

  async saveDocument() {
    const session = get().session;
    if (!session) return;
    set({ status: "正在保存…", error: null });
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
        lastSavedContent: get().documentContent,
      });
    } catch (error) {
      setError(set, error);
      set({ status: "保存失败" });
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
        lastSavedContent: content,
      }));
    } catch (error) {
      setError(set, error);
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
      lastSavedContent: content,
    }));
  },

  async generate(instruction) {
    const session = get().session;
    if (!session) return;
    set({
      generating: true,
      error: null,
      status: "生成中...",
      generationTaskId: null,
      streamingText: "",
    });
    try {
      const candidate = await desktop.generate(
        session.current_document.id,
        instruction,
        (taskId) => useAppStore.setState({ generationTaskId: taskId }),
        (delta) =>
          useAppStore.setState((state) => ({
            streamingText: state.streamingText + delta,
          })),
      );
      set({
        candidate,
        generating: false,
        generationTaskId: null,
        streamingText: "",
        status: "候选已生成",
      });
    } catch (error) {
      setError(set, error);
      set({
        generating: false,
        generationTaskId: null,
        streamingText: "",
        status: "",
      });
    }
  },

  async cancelGeneration() {
    const taskId = get().generationTaskId;
    if (!taskId) return;
    try {
      await desktop.generationCancel(taskId);
      set({
        generating: false,
        generationTaskId: null,
        streamingText: "",
        status: "已取消",
      });
    } catch (error) {
      setError(set, error);
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
        lastSavedContent: candidate.content,
      });
    } catch (error) {
      setError(set, error);
    }
  },

  async rejectCandidate() {
    const candidate = get().candidate;
    if (!candidate) return;
    try {
      await desktop.candidateReject(candidate.id);
      set({ candidate: null, status: "已拒绝" });
    } catch (error) {
      setError(set, error);
    }
  },

  async exportDocument(format) {
    set({ status: "导出中...", error: null });
    try {
      const lastExport = await desktop.exportDocument(format);
      set({ status: `已导出 ${format.toUpperCase()}`, lastExport });
    } catch (error) {
      setError(set, error);
    }
  },
}));
