import { render, screen } from "@testing-library/react";
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

async function createNoviceProject(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: /开始写作/ }));
  await user.type(screen.getByLabelText("作品名"), "我的小说");
  await user.click(screen.getByRole("button", { name: "创建项目" }));
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
    });
  });

  it("first launch shows exactly three primary actions", () => {
    render(<App />);
    expect(screen.getByRole("button", { name: /开始写作/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /打开已有作品/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /连接 AI/ })).toBeInTheDocument();
  });

  it("creates a project with only a name (no path knowledge needed)", async () => {
    const user = userEvent.setup();
    render(<App />);
    await createNoviceProject(user);

    expect(await screen.findByLabelText("编辑器")).toHaveValue(
      "# 第一章\n\n浏览器预览模式",
    );
    expect(screen.getByText("项目已创建")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存" })).toBeInTheDocument();
  });

  it("first chapter appears automatically after creation", async () => {
    const user = userEvent.setup();
    render(<App />);
    await createNoviceProject(user);

    expect(
      await screen.findByRole("button", { name: "第一章" }),
    ).toBeInTheDocument();
  });

  it("the custom save location is hidden behind advanced options", () => {
    render(<App />);
    expect(screen.queryByLabelText("自定义保存位置")).not.toBeInTheDocument();
  });

  it("generates a candidate and adopts it into the editor", async () => {
    const user = userEvent.setup();
    render(<App />);
    await createNoviceProject(user);
    await user.click(screen.getByRole("button", { name: "生成" }));

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
    await createNoviceProject(user);
    await user.click(screen.getByRole("button", { name: "新建章节" }));

    expect(
      await screen.findByRole("button", { name: "第二章" }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "第二章" }));
    expect(screen.getByLabelText("编辑器")).toHaveValue("");
    await user.click(screen.getByRole("button", { name: "第一章" }));
    expect(screen.getByLabelText("编辑器")).toHaveValue(
      "# 第一章\n\n浏览器预览模式",
    );
  });

  it("shows human-readable guidance for typed errors", () => {
    expect(humanizeError("AiAuthFailed").title).toBe("API Key 无效");
    expect(humanizeError("AiAuthFailed").guidance).toContain("API Key");
    expect(humanizeError("AiNetworkError").title).toBe("网络连接失败");
    expect(humanizeError("DocumentConflict").guidance).toContain("没有覆盖");
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
    ];
    for (const code of codes) {
      const human = humanizeError(code);
      expect(human.title.length).toBeGreaterThan(0);
      expect(human.guidance.length).toBeGreaterThan(0);
      expect(human.title).not.toMatch(/[A-Za-z]{4,}/, `${code} leaks raw code`);
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

  it("keeps the save status visible", async () => {
    const user = userEvent.setup();
    render(<App />);
    await createNoviceProject(user);
    expect(await screen.findByText("已保存")).toBeInTheDocument();

    const editor = screen.getByLabelText("编辑器");
    await user.clear(editor);
    await user.type(editor, "雨夜");
    expect(screen.getByText("未保存")).toBeInTheDocument();
  });
});
