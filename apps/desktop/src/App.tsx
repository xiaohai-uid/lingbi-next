import { useState } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { useAppStore } from "./store/useAppStore";

function Welcome() {
  const [name, setName] = useState("测试小说");
  const [root, setRoot] = useState("");
  const createProject = useAppStore((state) => state.createProject);
  const openProject = useAppStore((state) => state.openProject);
  const status = useAppStore((state) => state.status);
  const error = useAppStore((state) => state.error);

  return (
    <main className="welcome">
      <section className="panel">
        <h1>LingBi Next</h1>
        <label>
          项目名
          <input value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        <label>
          项目目录
          <input
            placeholder="/home/user/MyNovel"
            value={root}
            onChange={(event) => setRoot(event.target.value)}
          />
        </label>
        <div className="actions">
          <button onClick={() => createProject(name, root || name)}>创建项目</button>
          <button onClick={() => openProject(root || name)}>打开项目</button>
        </div>
        {status ? <p className="status">{status}</p> : null}
        {error ? <p className="error">{error}</p> : null}
      </section>
    </main>
  );
}

function Editor() {
  const session = useAppStore((state) => state.session);
  const documentContent = useAppStore((state) => state.documentContent);
  const status = useAppStore((state) => state.status);
  const error = useAppStore((state) => state.error);
  const saveDocument = useAppStore((state) => state.saveDocument);
  const selectDocument = useAppStore((state) => state.selectDocument);
  const setDocumentContent = (content: string) =>
    useAppStore.setState({ documentContent: content, session: session ? { ...session, dirty: true } : session });

  if (!session) return <Welcome />;

  return (
    <main className="editor-shell">
      <aside className="chapters">
        <h2>章节</h2>
        <button
          className={session.current_document.id ? "active" : ""}
          onClick={() => selectDocument(session.current_document)}
        >
          {session.current_document.title}
        </button>
      </aside>
      <section className="editor">
        <header>
          <strong>{session.current_document.title}</strong>
          <span>{status || (session.dirty ? "未保存" : "已保存")}</span>
        </header>
        <CodeMirror
          value={documentContent}
          height="calc(100vh - 96px)"
          extensions={[markdown()]}
          onChange={setDocumentContent}
        />
        <footer>
          <button onClick={saveDocument}>保存</button>
          {error ? <span className="error">{error}</span> : null}
        </footer>
      </section>
      <aside className="candidate">
        <h2>候选</h2>
        <p>AI 生成后在此确认</p>
        <button disabled>采纳</button>
        <button disabled>拒绝</button>
      </aside>
    </main>
  );
}

export default function App() {
  const selectedTab = useAppStore((state) => state.selectedTab);
  return selectedTab === "welcome" ? <Welcome /> : <Editor />;
}
