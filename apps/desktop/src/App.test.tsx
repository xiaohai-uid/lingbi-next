import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import App from "./App";

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
});
