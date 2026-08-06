import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
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

describe("LingBi Next desktop shell", () => {
  beforeEach(() => {
    useAppStore.setState({
      session: null,
      documentContent: "",
      candidate: null,
      generating: false,
      status: "",
      error: null,
      selectedTab: "welcome",
    });
  });

  it("creates a project and opens the editor", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.clear(screen.getByLabelText("项目名"));
    await user.type(screen.getByLabelText("项目名"), "测试小说");
    await user.clear(screen.getByLabelText("项目目录"));
    await user.type(screen.getByLabelText("项目目录"), "/tmp/MyNovel");
    await user.click(screen.getByRole("button", { name: "创建项目" }));

    expect(await screen.findByLabelText("编辑器")).toHaveValue(
      "# 第一章\n\n浏览器预览模式",
    );
    expect(screen.getByText("项目已创建")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存" })).toBeInTheDocument();
  });

  it("generates a candidate and adopts it into the editor", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.clear(screen.getByLabelText("项目名"));
    await user.type(screen.getByLabelText("项目名"), "测试小说");
    await user.clear(screen.getByLabelText("项目目录"));
    await user.type(screen.getByLabelText("项目目录"), "/tmp/MyNovel");
    await user.click(screen.getByRole("button", { name: "创建项目" }));
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
});
