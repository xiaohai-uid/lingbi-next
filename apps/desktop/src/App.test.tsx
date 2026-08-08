import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { humanizeError } from "./App";
import { useAppStore } from "./store/useAppStore";

vi.mock("@uiw/react-codemirror", () => ({
  default: ({
    value,
    onChange,
  }: {
    value: string;
    onChange: (value: string) => void;
  }) => (
    <textarea
      aria-label="编辑器"
      value={value}
      onChange={(event) => onChange(event.target.value)}
    />
  ),
}));

/// Book V3 建书（P5-1）：开始写作 → 作品名 → 创建项目（create_book 命令）。
async function createNoviceBook(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: /开始写作/ }));
  await user.type(screen.getByLabelText("作品名"), "我的小说");
  await user.click(screen.getByRole("button", { name: "创建项目" }));
}

async function createBookWithChapter(user: ReturnType<typeof userEvent.setup>) {
  await createNoviceBook(user);
  await user.type(screen.getByLabelText("新章节标题"), "第一章");
  await user.click(screen.getByRole("button", { name: "新建章节" }));
  await screen.findByRole("button", { name: "第一章" });
}

describe("LingBi Next desktop shell", () => {
  beforeEach(() => {
    useAppStore.setState({
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
      // Book V3（P5-1）
      book: null,
      bookChapters: [],
      currentCh: null,
      chapterContent: "",
      chapterHash: "",
      saveState: "idle",
      bookGenerating: false,
      bookGenerationTaskId: null,
      bookStreamingText: "",
      bookCandidate: null,
      bookOutline: [],
      bookContract: "",
      bookBrief: "",
      bookChecks: [],
      bookDiagnose: "",
      toolsLoading: false,
    });
  });

  it("first launch shows exactly three primary actions", () => {
    render(<App />);
    expect(screen.getByRole("button", { name: /开始写作/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /打开已有作品/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /连接 AI/ })).toBeInTheDocument();
  });

  it("creates a book with only a name (no path knowledge needed)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await createNoviceBook(user);

    expect(await screen.findByText("书仓已创建")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存" })).toBeInTheDocument();
    // Book V3 建书后无自动章——首章由「新建章节」创建
    expect(screen.queryByRole("button", { name: "第一章" })).not.toBeInTheDocument();
  });

  it("creates a first chapter after book creation", async () => {
    const user = userEvent.setup();
    render(<App />);
    await createNoviceBook(user);

    await user.type(screen.getByLabelText("新章节标题"), "第一章");
    await user.click(screen.getByRole("button", { name: "新建章节" }));

    expect(
      await screen.findByRole("button", { name: "第一章" }),
    ).toBeInTheDocument();
    expect(screen.getByText("章节已创建")).toBeInTheDocument();
  });

  it("generates a candidate and adopts it into the editor", async () => {
    // V2 生成流（P5-2 迁移前保留）：直接构造 V2 会话
    const session = {
      root: "/p/novel",
      dirty: false,
      recovered: false,
      protected: false,
      project: {
        id: "p1",
        name: "小说",
        schema_version: 2,
        created_at: "",
        updated_at: "",
      },
      current_document: {
        id: "d1",
        project_id: "p1",
        title: "第一章",
        order: 0,
        revision: 0,
        content_hash: "",
        created_at: "",
        updated_at: "",
      },
    };
    useAppStore.setState({
      session,
      documents: [session.current_document],
      documentContent: "# 第一章\n\n浏览器预览模式",
      selectedTab: "editor",
      lastSavedContent: "# 第一章\n\n浏览器预览模式",
    });
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: "生成" }));
    // mock 流式为异步（delta + resolve），等待候选渲染
    await new Promise((resolve) => setTimeout(resolve, 200));

    expect(
      await screen.findByText("第一章正文：雨夜，林渊推开旧车站的门。"),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "采纳" }));

    expect(screen.getByText("已采纳")).toBeInTheDocument();
    expect(screen.getByLabelText("编辑器")).toHaveValue(
      "第一章正文：雨夜，林渊推开旧车站的门。",
    );
  });

  it("creates a second chapter and switches between chapters", async () => {
    const user = userEvent.setup();
    render(<App />);
    await createBookWithChapter(user);

    await user.type(screen.getByLabelText("新章节标题"), "第二章");
    await user.click(screen.getByRole("button", { name: "新建章节" }));

    expect(
      await screen.findByRole("button", { name: "第二章" }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "第二章" }));
    expect(screen.getByLabelText("编辑器")).toHaveValue("");
    await user.click(screen.getByRole("button", { name: "第一章" }));
    expect(screen.getByLabelText("编辑器")).toHaveValue("");
  });

  it("the custom save location is hidden behind advanced options", () => {
    render(<App />);
    expect(screen.queryByLabelText("自定义保存位置")).not.toBeInTheDocument();
  });

  it("shows human-readable guidance for typed errors", () => {
    expect(humanizeError("AiAuthFailed").title).toBe("API Key 无效");
    expect(humanizeError("AiAuthFailed").guidance).toContain("API Key");
    expect(humanizeError("AiNetworkError").title).toBe("网络连接失败");
    expect(humanizeError("DocumentConflict").guidance).toContain("没有覆盖");
  });

  it("高级设置 exposes 协议/温度/最大 tokens and persists them", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: /连接 AI/ }));
    await user.type(screen.getByLabelText("API Key"), "sk-test");

    // Fields are inside the collapsed 高级设置 details until expanded.
    const details = screen.getByText("高级设置").closest("details");
    expect(details).not.toBeNull();
    expect(details!.hasAttribute("open")).toBe(false);
    const summary = screen.getByText("高级设置");
    await user.click(summary);
    expect(details!.hasAttribute("open")).toBe(true);

    expect(screen.getByLabelText("协议")).toHaveValue("OpenAI 兼容");
    await user.clear(screen.getByLabelText("温度"));
    await user.type(screen.getByLabelText("温度"), "0.3");
    await user.clear(screen.getByLabelText("最大 tokens"));
    await user.type(screen.getByLabelText("最大 tokens"), "123");

    await user.click(screen.getByRole("button", { name: "保存设置" }));

    const { lastProviderConfig } = await import("./lib/desktop");
    expect(lastProviderConfig?.temperature).toBe(0.3);
    expect(lastProviderConfig?.maxTokens).toBe(123);
    expect(lastProviderConfig?.providerId).toBe("openai");
  });

  it("高级设置回显：重启后已存温度/tokens 预填（P5-5）", async () => {
    const { lastProviderConfig } = await import("./lib/desktop");
    // 模拟上一次保存的持久化值（mock provider_status 读回）
    expect(lastProviderConfig).not.toBeNull();
    lastProviderConfig!.temperature = 0.3;
    lastProviderConfig!.maxTokens = 123;
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("button", { name: /连接 AI/ }));
    await user.click(screen.getByText("高级设置"));
    expect((screen.getByLabelText("温度") as HTMLInputElement).value).toBe("0.3");
    expect((screen.getByLabelText("最大 tokens") as HTMLInputElement).value).toBe("123");
  });

  it("maps every typed AI/disk error to Chinese guidance with a next step", () => {
    const codes = [
      "AiAuthFailed",
      "AiNoApiKey",
      "AiRateLimited",
      "AiServerError",
      "AiTimeout",
      "AiNetworkError",
      "AiInvalidResponse",
      "AiCancelled",
      "DocumentConflict",
      "CandidateStale",
      "DiskWriteFailed",
    ];
    for (const code of codes) {
      const human = humanizeError(code);
      expect(human.title.length).toBeGreaterThan(0);
      expect(human.guidance.length).toBeGreaterThan(0);
      expect(human.title).not.toMatch(/[A-Za-z]{4,}/);
    }
  });

  it("renders the next-step button for a typed error", () => {
    useAppStore.setState({
      session: null,
      selectedTab: "welcome",
      error: "authentication failed",
      errorCode: "AiAuthFailed",
    });
    render(<App />);
    expect(screen.getByText("API Key 无效")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新配置" })).toBeInTheDocument();
  });

  it("shows the protective dialog when recovery preserved user bytes", () => {
    const session = {
      root: "/p/novel",
      dirty: false,
      recovered: false,
      protected: true,
      project: {
        id: "p1",
        name: "小说",
        schema_version: 2,
        created_at: "",
        updated_at: "",
      },
      current_document: {
        id: "d1",
        project_id: "p1",
        title: "第一章",
        order: 0,
        revision: 0,
        content_hash: "",
        created_at: "",
        updated_at: "",
      },
    };
    useAppStore.setState({
      session,
      documents: [session.current_document],
      selectedTab: "editor",
      recoveryDismissed: false,
    });
    render(<App />);

    expect(screen.getByText("检测到上次异常关闭")).toBeInTheDocument();
    expect(screen.getByText("你的当前正文已经保护，没有被覆盖。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保留当前版本" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查看恢复内容" })).toBeInTheDocument();
    // No internal jargon for novices.
    expect(screen.queryByText(/CommitIntent|SHA256|MutationIncident/)).toBeNull();
  });

  it("查看恢复内容 shows the recovered AI content instead of dismissing", async () => {
    const session = {
      root: "/p/novel",
      dirty: false,
      recovered: false,
      protected: true,
      project: {
        id: "p1",
        name: "小说",
        schema_version: 2,
        created_at: "",
        updated_at: "",
      },
      current_document: {
        id: "d1",
        project_id: "p1",
        title: "第一章",
        order: 0,
        revision: 0,
        content_hash: "",
        created_at: "",
        updated_at: "",
      },
    };
    useAppStore.setState({
      session,
      documents: [session.current_document],
      selectedTab: "editor",
      recoveryDismissed: false,
      recoveryCandidate: null,
    });
    const { desktop } = await import("./lib/desktop");
    await desktop.generate("d1", "恢复测试");

    render(<App />);
    await userEvent.setup().click(
      screen.getByRole("button", { name: "查看恢复内容" }),
    );

    expect(
      await screen.findByText("第一章正文：雨夜，林渊推开旧车站的门。"),
    ).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "恢复内容" })).toBeInTheDocument();
  });

  it("shows the recovered banner after safe recovery", () => {
    const session = {
      root: "/p/novel",
      dirty: false,
      recovered: true,
      protected: false,
      project: {
        id: "p1",
        name: "小说",
        schema_version: 2,
        created_at: "",
        updated_at: "",
      },
      current_document: {
        id: "d1",
        project_id: "p1",
        title: "第一章",
        order: 0,
        revision: 0,
        content_hash: "",
        created_at: "",
        updated_at: "",
      },
    };
    useAppStore.setState({
      session,
      documents: [session.current_document],
      selectedTab: "editor",
      recoveryDismissed: false,
    });
    render(<App />);

    expect(screen.getByText("已恢复上次未完成的保存")).toBeInTheDocument();
  });

  it("keeps the save status visible (Book V3)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await createBookWithChapter(user);
    expect(await screen.findByText("已保存")).toBeInTheDocument();

    const editor = screen.getByLabelText("编辑器");
    await user.clear(editor);
    await user.type(editor, "雨夜");
    // 编辑后未保存（自动保存 2s 防抖）
    expect(screen.getByText("未保存")).toBeInTheDocument();
  });

  it("restarts and reopens the book with content intact (Book V3)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await createBookWithChapter(user);

    // 写正文并等待自动保存落盘（mock saveChapter）
    const editor = screen.getByLabelText("编辑器");
    await user.type(editor, "第一章正文：雨夜，林渊推开旧车站的门。");
    await screen.findByText("已保存", undefined, { timeout: 4000 });

    // 模拟重启：回到欢迎页（store 清空）+ 打开最近
    act(() => {
      useAppStore.setState({
        book: null,
        bookChapters: [],
        currentCh: null,
        chapterContent: "",
        chapterHash: "",
        saveState: "idle",
        selectedTab: "welcome",
      });
    });
    await user.click(screen.getByRole("button", { name: /打开已有作品/ }));
    await user.click(screen.getByText("我的小说"));

    // 书仓重开 → 打开第一章 → 正文完整
    expect(await screen.findByText("书仓已打开")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "第一章" }));
    expect(screen.getByLabelText("编辑器")).toHaveValue(
      "第一章正文：雨夜，林渊推开旧车站的门。",
    );
  });

  it("shows conflict guidance when an external edit blocks saving", async () => {
    const user = userEvent.setup();
    render(<App />);
    await createBookWithChapter(user);

    // 外部修改：绕过 UI 直接改 mock 的章内容（哈希随之变化）
    const { desktop } = await import("./lib/desktop");
    await desktop.saveChapter(1, "外部修改后的内容");

    // 编辑器仍持有旧哈希 → 保存被拒 → conflict 文案
    await user.type(screen.getByLabelText("编辑器"), "作者的新正文");
    await user.click(screen.getByRole("button", { name: "保存" }));
    expect(
      await screen.findByText("保存失败：内容已被外部修改，请重新打开章节"),
    ).toBeInTheDocument();

    // 重新打开章节 → 恢复磁盘内容（用户字节保留）
    await user.click(screen.getByRole("button", { name: "第一章" }));
    expect(screen.getByLabelText("编辑器")).toHaveValue("外部修改后的内容");
  });

  it("flushes unsaved edits before switching chapters", async () => {
    const user = userEvent.setup();
    render(<App />);
    await createBookWithChapter(user);

    // 输入后立即（防抖窗口内）切到第二章——编辑必须先落盘
    await user.type(screen.getByLabelText("编辑器"), "第一章的未保存编辑");
    await user.type(screen.getByLabelText("新章节标题"), "第二章");
    await user.click(screen.getByRole("button", { name: "新建章节" }));

    // 切回第一章 → 编辑内容完整（切章前已 flush）
    await user.click(screen.getByRole("button", { name: "第一章" }));
    expect(screen.getByLabelText("编辑器")).toHaveValue(
      "第一章的未保存编辑",
    );
  });

  it("generates a Book candidate and adopts it into the editor", async () => {
    const user = userEvent.setup();
    render(<App />);
    await createBookWithChapter(user);

    await user.click(screen.getByRole("button", { name: "生成" }));
    // mock 流式为异步（delta + resolve），等待候选渲染
    await new Promise((resolve) => setTimeout(resolve, 200));

    expect(
      await screen.findByText("第一章正文：雨夜，林渊推开旧车站的门。"),
    ).toBeInTheDocument();
    expect(await screen.findByText("候选内容")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "采纳" }));

    expect(await screen.findByText("已采纳")).toBeInTheDocument();
    // 采纳后正文 = 候选内容（mock bookAdopt 写入章节）
    expect(screen.getByLabelText("编辑器")).toHaveValue(
      "第一章正文：雨夜，林渊推开旧车站的门。",
    );
    // 候选已清空（再次生成需重新点生成）
    expect(screen.queryByRole("button", { name: "拒绝" })).not.toBeInTheDocument();
  });

  it("loads writing tools (outline/contract/brief/checks/diagnose)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await createBookWithChapter(user);

    // 大纲（mock 空表 → 提示）
    await user.click(screen.getByText("大纲"));
    expect(await screen.findByText("暂无细纲蓝图")).toBeInTheDocument();

    // 合同（mock JSON）
    await user.click(screen.getByText("合同"));
    expect(await screen.findByText(/goal/)).toBeInTheDocument();

    // 任务书（mock markdown）
    await user.click(screen.getByText("任务书"));
    expect(await screen.findByText(/第一段写作委托/)).toBeInTheDocument();

    // 机检（mock 11 项全过）
    await user.click(screen.getByText("机检"));
    expect(await screen.findByText("✔ 机检1")).toBeInTheDocument();

    // 断点诊断（mock JSON）
    await user.click(screen.getByText("断点诊断"));
    expect(await screen.findByText(/status/)).toBeInTheDocument();

    // 技能（P5-5 声明式只读卡：mock 两卡 + P9 占位提示）
    await user.click(screen.getByText("技能"));
    expect(await screen.findByText("题材引导（精简版）")).toBeInTheDocument();
    expect(screen.getByText("章节骨架")).toBeInTheDocument();
    expect(screen.getByText(/4 步写作模式（P9 上线）/)).toBeInTheDocument();
  });

  it("exports the open chapter (Book V3)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await createBookWithChapter(user);

    await user.click(screen.getByRole("button", { name: "导出 MD" }));
    expect(await screen.findByText(/已导出 MD/)).toBeInTheDocument();
    expect(screen.getByTitle(/export\//)).toBeInTheDocument();
  });

  it("rejects a Book candidate and keeps the chapter untouched", async () => {
    const user = userEvent.setup();
    render(<App />);
    await createBookWithChapter(user);

    await user.click(screen.getByRole("button", { name: "生成" }));
    await screen.findByText("候选内容");
    await user.click(screen.getByRole("button", { name: "拒绝" }));

    expect(await screen.findByText("已拒绝")).toBeInTheDocument();
    expect(screen.getByLabelText("编辑器")).toHaveValue("");
  });
});
