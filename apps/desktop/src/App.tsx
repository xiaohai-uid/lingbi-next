import { useEffect, useState } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { markdown } from "@codemirror/lang-markdown";
import { useAppStore } from "./store/useAppStore";
import type { CommandError } from "./lib/desktop";

interface HumanError {
  title: string;
  guidance: string;
  action: "retry" | "reconfigure" | "switch_model" | "keep_current" | "regenerate";
}

/** Task 9: every AI/disk failure becomes "what happened + what to do next". */
export function humanizeError(code: string): HumanError {
  switch (code) {
    case "AiAuthFailed":
      return {
        title: "API Key 无效",
        guidance: "请重新复制完整的 API Key，再试一次。",
        action: "reconfigure",
      };
    case "AiNoApiKey":
      return {
        title: "还没有配置 AI",
        guidance: "先选择 AI 服务并粘贴 API Key，然后测试连接。",
        action: "reconfigure",
      };
    case "AiRateLimited":
      return {
        title: "请求太频繁",
        guidance: "模型暂时繁忙，可以稍后重试或切换模型。",
        action: "switch_model",
      };
    case "AiServerError":
      return {
        title: "模型暂时繁忙",
        guidance: "可以稍后重试或切换模型。",
        action: "switch_model",
      };
    case "AiTimeout":
    case "AiNetworkError":
      return {
        title: "网络连接失败",
        guidance: "请检查网络后重试。",
        action: "retry",
      };
    case "AiInvalidResponse":
      return {
        title: "AI 返回了无法理解的内容",
        guidance: "请重试，或切换模型。",
        action: "retry",
      };
    case "AiCancelled":
      return {
        title: "已取消生成",
        guidance: "生成已停止，你可以继续写作。",
        action: "retry",
      };
    case "DocumentConflict":
      return {
        title: "正文已在 AI 生成期间被修改",
        guidance: "为了保护你的内容，LingBi 没有覆盖当前正文。",
        action: "keep_current",
      };
    case "CandidateStale":
      return {
        title: "正文已经变化",
        guidance: "这条候选基于旧正文，请重新生成。",
        action: "regenerate",
      };
    default:
      return {
        title: "操作没有成功",
        guidance: "请稍后重试。",
        action: "retry",
      };
  }
}

function ErrorPanel({ error }: { error: CommandError }) {
  const human = humanizeError(error.code);
  const dismiss = () => useAppStore.setState({ error: null, errorCode: null, status: "" });
  return (
    <div className="error-panel" role="alert">
      <strong>{human.title}</strong>
      <p>{human.guidance}</p>
      <p className="error-detail">{error.message}</p>
      {human.action === "reconfigure" || human.action === "switch_model" ? (
        <button onClick={() => useAppStore.setState({ selectedTab: "welcome", error: null, errorCode: null })}>
          {human.action === "switch_model" ? "切换模型" : "重新配置"}
        </button>
      ) : (
        <button onClick={dismiss}>知道了</button>
      )}
    </div>
  );
}

type WelcomeView = "home" | "create" | "open" | "ai";

function Welcome() {
  const [view, setView] = useState<WelcomeView>("home");
  const [name, setName] = useState("");
  const [customRoot, setCustomRoot] = useState("");
  const [useCustomRoot, setUseCustomRoot] = useState(false);
  const [defaultRoot, setDefaultRoot] = useState("");
  const [openRoot, setOpenRoot] = useState("");
  const [providerId, setProviderId] = useState("openai");
  const [apiKey, setApiKey] = useState("");
  const [advancedAi, setAdvancedAi] = useState(false);
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");

  const createProject = useAppStore((state) => state.createProject);
  const openProject = useAppStore((state) => state.openProject);
  const loadRecent = useAppStore((state) => state.loadRecent);
  const loadProviders = useAppStore((state) => state.loadProviders);
  const recentProjects = useAppStore((state) => state.recentProjects);
  const providers = useAppStore((state) => state.providers);
  const status = useAppStore((state) => state.status);
  const error = useAppStore((state) => state.error);
  const errorCode = useAppStore((state) => state.errorCode);
  const providerTest = useAppStore((state) => state.providerTest);
  const providerConfigure = useAppStore((state) => state.providerConfigure);
  const testProvider = useAppStore((state) => state.testProvider);
  const aiConfigured = useAppStore((state) => state.aiConfigured);

  useEffect(() => {
    void loadRecent();
    void loadProviders();
  }, [loadRecent, loadProviders]);

  useEffect(() => {
    let cancelled = false;
    if (view === "create" && name.trim()) {
      const timeout = window.setTimeout(() => {
        desktopDefaultRoot(name).then((root) => {
          if (!cancelled) setDefaultRoot(root);
        });
      }, 150);
      return () => {
        cancelled = true;
        window.clearTimeout(timeout);
      };
    }
  }, [view, name]);

  async function desktopDefaultRoot(value: string): Promise<string> {
    const { desktop } = await import("./lib/desktop");
    return desktop.projectDefaultRoot(value);
  }

  function onOpenRecent(root: string) {
    void openProject(root);
  }

  const selectedProvider =
    providers.find((provider) => provider.id === providerId) ?? null;

  return (
    <main className="welcome">
      <section className="panel welcome-panel">
        <header className="brand">
          <h1>LingBi Next</h1>
          <p>你的中文小说写作助手</p>
        </header>

        {view === "home" ? (
          <div className="launch-actions">
            <button className="launch-card" onClick={() => setView("create")}>
              <strong>开始写作</strong>
              <span>创建一本新小说</span>
            </button>
            <button className="launch-card" onClick={() => setView("open")}>
              <strong>打开已有作品</strong>
              <span>继续写最近的小说</span>
            </button>
            <button className="launch-card" onClick={() => setView("ai")}>
              <strong>连接 AI</strong>
              <span>粘贴 API Key 即可开始</span>
            </button>
          </div>
        ) : null}

        {view === "create" ? (
          <div className="welcome-form">
            <label>
              作品名
              <input
                aria-label="作品名"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="例如：我的第一本小说"
              />
            </label>
            {!useCustomRoot ? (
              <p className="hint">
                将保存到 {defaultRoot || `文档/LingBi/${name.trim() || "我的小说"}`}
              </p>
            ) : null}
            <details className="advanced">
              <summary>高级选项</summary>
              <label>
                自定义保存位置
                <input
                  aria-label="自定义保存位置"
                  value={customRoot}
                  onChange={(event) => {
                    setCustomRoot(event.target.value);
                    setUseCustomRoot(true);
                  }}
                  placeholder="留空则使用默认位置"
                />
              </label>
            </details>
            <div className="actions">
              <button
                disabled={!name.trim()}
                onClick={() =>
                  void createProject(name.trim(), useCustomRoot ? customRoot : undefined)
                }
              >
                创建项目
              </button>
              <button onClick={() => setView("home")}>返回</button>
            </div>
          </div>
        ) : null}

        {view === "open" ? (
          <div className="welcome-form">
            <h2>最近作品</h2>
            {recentProjects.length === 0 ? (
              <p className="hint">还没有最近作品</p>
            ) : (
              <ul className="recent-list">
                {recentProjects.map((project) => (
                  <li key={project.root}>
                    <button onClick={() => onOpenRecent(project.root)}>
                      <strong>{project.name}</strong>
                      <span>{project.root}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <details className="advanced">
              <summary>高级选项</summary>
              <label>
                项目路径
                <input
                  aria-label="项目路径"
                  value={openRoot}
                  onChange={(event) => setOpenRoot(event.target.value)}
                  placeholder="输入完整路径打开"
                />
              </label>
              <div className="actions">
                <button disabled={!openRoot.trim()} onClick={() => void openProject(openRoot.trim())}>
                  打开
                </button>
              </div>
            </details>
            <div className="actions">
              <button onClick={() => setView("home")}>返回</button>
            </div>
          </div>
        ) : null}

        {view === "ai" ? (
          <div className="welcome-form">
            <h2>连接 AI</h2>
            <label>
              选择 AI 服务
              <select
                aria-label="选择 AI 服务"
                value={providerId}
                onChange={(event) => {
                  setProviderId(event.target.value);
                  setModel("");
                  setBaseUrl("");
                }}
              >
                {providers.map((provider) => (
                  <option key={provider.id} value={provider.id}>
                    {provider.display_name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              API Key
              <input
                aria-label="API Key"
                type="password"
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
                placeholder="粘贴你的 API Key"
              />
            </label>
            {selectedProvider ? (
              <p className="hint">
                推荐模型：{selectedProvider.recommended_model}
              </p>
            ) : null}
            <details
              className="advanced"
              open={advancedAi}
              onToggle={(event) => setAdvancedAi(event.currentTarget.open)}
            >
              <summary>高级设置</summary>
              <label>
                自定义 Base URL
                <input
                  aria-label="自定义 Base URL"
                  value={baseUrl}
                  onChange={(event) => setBaseUrl(event.target.value)}
                />
              </label>
              <label>
                自定义模型 ID
                <input
                  aria-label="自定义模型 ID"
                  value={model}
                  onChange={(event) => setModel(event.target.value)}
                />
              </label>
            </details>
            <div className="actions">
              <button
                disabled={!apiKey.trim()}
                onClick={() =>
                  void providerConfigure(
                    providerId,
                    apiKey.trim(),
                    advancedAi && baseUrl.trim() ? baseUrl.trim() : undefined,
                    advancedAi && model.trim() ? model.trim() : undefined,
                  )
                }
              >
                保存设置
              </button>
              <button
                disabled={!aiConfigured && !apiKey.trim()}
                onClick={() => void testProvider()}
              >
                测试连接
              </button>
            </div>
            {providerTest ? (
              <p className={providerTest.ok ? "status" : "error"}>
                {providerTest.ok
                  ? `连接成功（${providerTest.latency_ms}ms）`
                  : `连接失败：${providerTest.error ?? "未知错误"}`}
              </p>
            ) : null}
            <div className="actions">
              <button onClick={() => setView("home")}>返回</button>
            </div>
          </div>
        ) : null}

        {status ? <p className="status">{status}</p> : null}
        {error ? (
          <ErrorPanel
            error={{
              code: errorCode ?? "UNKNOWN",
              message: error,
              retryable: false,
            }}
          />
        ) : null}
      </section>
    </main>
  );
}

function RecoveryBanner() {
  const session = useAppStore((state) => state.session);
  const recoveryDismissed = useAppStore((state) => state.recoveryDismissed);
  const dismissRecovery = useAppStore((state) => state.dismissRecovery);
  const loadRecoveryCandidate = useAppStore(
    (state) => state.loadRecoveryCandidate,
  );
  const recoveryCandidate = useAppStore((state) => state.recoveryCandidate);
  if (!session) return null;

  const banner =
    !recoveryDismissed && session.protected ? (
      <div className="recovery-banner protected" role="status">
        <strong>检测到上次异常关闭</strong>
        <p>你的当前正文已经保护，没有被覆盖。</p>
        <div className="actions">
          <button onClick={dismissRecovery}>保留当前版本</button>
          <button onClick={() => void loadRecoveryCandidate()}>
            查看恢复内容
          </button>
        </div>
      </div>
    ) : !recoveryDismissed && session.recovered ? (
      <div className="recovery-banner" role="status">
        <strong>已恢复上次未完成的保存</strong>
        <button onClick={dismissRecovery}>知道了</button>
      </div>
    ) : null;

  const dialog = recoveryCandidate ? (
    <div className="recovery-dialog" role="dialog" aria-label="恢复内容">
      <div>
        <strong>上次 AI 生成的内容</strong>
        <p>{recoveryCandidate.content}</p>
        <div className="actions">
          <button
            onClick={() => useAppStore.setState({ recoveryCandidate: null })}
          >
            关闭
          </button>
        </div>
      </div>
    </div>
  ) : null;

  return (
    <>
      {banner}
      {dialog}
    </>
  );
}

function SaveStatus() {
  const session = useAppStore((state) => state.session);
  const status = useAppStore((state) => state.status);
  const error = useAppStore((state) => state.error);
  if (status === "正在保存…") return <span className="status">正在保存…</span>;
  if (status === "已保存") return <span className="status">已保存</span>;
  if (status === "保存失败" || error) return <span className="error">保存失败</span>;
  if (session?.dirty) return <span className="unsaved">未保存</span>;
  return <span className="status">已保存</span>;
}

function Editor() {
  const [instruction, setInstruction] = useState("生成一个雨夜开场");
  const session = useAppStore((state) => state.session);
  const documents = useAppStore((state) => state.documents);
  const documentContent = useAppStore((state) => state.documentContent);
  const candidate = useAppStore((state) => state.candidate);
  const generating = useAppStore((state) => state.generating);
  const status = useAppStore((state) => state.status);
  const error = useAppStore((state) => state.error);
  const errorCode = useAppStore((state) => state.errorCode);
  const streamingText = useAppStore((state) => state.streamingText);
  const saveDocument = useAppStore((state) => state.saveDocument);
  const generate = useAppStore((state) => state.generate);
  const cancelGeneration = useAppStore((state) => state.cancelGeneration);
  const adoptCandidate = useAppStore((state) => state.adoptCandidate);
  const rejectCandidate = useAppStore((state) => state.rejectCandidate);
  const selectDocument = useAppStore((state) => state.selectDocument);
  const createChapter = useAppStore((state) => state.createChapter);
  const exportDocument = useAppStore((state) => state.exportDocument);
  const lastExport = useAppStore((state) => state.lastExport);
  const setDocumentContent = (content: string) =>
    useAppStore.setState({
      documentContent: content,
      session: session ? { ...session, dirty: true } : session,
    });

  // Safe autosave: short debounce, revision/hash-guarded save (never
  // overwrites external edits), and a no-op when the content already
  // matches the last saved state.
  useEffect(() => {
    const session = useAppStore.getState().session;
    const lastSavedContent = useAppStore.getState().lastSavedContent;
    if (!session || documentContent === lastSavedContent) return;
    const timer = window.setTimeout(() => {
      const state = useAppStore.getState();
      if (state.session && state.documentContent !== state.lastSavedContent) {
        void state.saveDocument();
      }
    }, 2000);
    return () => window.clearTimeout(timer);
  }, [documentContent]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        void saveDocument();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [saveDocument]);

  if (!session) return <Welcome />;

  return (
    <main className="editor-shell">
      <aside className="chapters">
        <h2>章节</h2>
        {documents.map((document) => (
          <button
            key={document.id}
            className={
              document.id === session.current_document.id ? "active" : ""
            }
            onClick={() => selectDocument(document)}
          >
            {document.title}
          </button>
        ))}
        <button onClick={createChapter}>新建章节</button>
        <details className="advanced">
          <summary>高级功能</summary>
          <button onClick={() => useAppStore.setState({ selectedTab: "welcome" })}>
            返回首页
          </button>
        </details>
      </aside>
      <section className="editor">
        <RecoveryBanner />
        <header>
          <strong>{session.current_document.title}</strong>
          <div className="header-status">
            {status &&
            status !== "正在保存…" &&
            status !== "已保存" &&
            status !== "保存失败" ? (
              <span className="hint">{status}</span>
            ) : null}
            <SaveStatus />
          </div>
        </header>
        <CodeMirror
          value={documentContent}
          height="calc(100vh - 96px)"
          extensions={[markdown()]}
          onChange={setDocumentContent}
        />
        <footer>
          <button onClick={saveDocument}>保存</button>
          <button onClick={() => void exportDocument("docx")}>导出 DOCX</button>
          <button onClick={() => void exportDocument("md")}>导出 MD</button>
          <button onClick={() => void exportDocument("txt")}>导出 TXT</button>
          {lastExport ? (
            <span className="hint" title={lastExport.path}>
              已导出到 {lastExport.path}
            </span>
          ) : null}
          <input
            aria-label="写作要求"
            value={instruction}
            onChange={(event) => setInstruction(event.target.value)}
          />
          <button disabled={generating} onClick={() => generate(instruction)}>
            {generating ? "生成中" : "生成"}
          </button>
          {generating ? <button onClick={cancelGeneration}>取消</button> : null}
        </footer>
        {error ? (
          <ErrorPanel
            error={{
              code: errorCode ?? "UNKNOWN",
              message: error,
              retryable: false,
            }}
          />
        ) : null}
      </section>
      <aside className="candidate">
        <h2>AI 助手</h2>
        {generating && streamingText ? (
          <>
            <p className="streaming">{streamingText}</p>
            <p className="hint">正在实时输出…</p>
          </>
        ) : null}
        {candidate ? (
          <>
            <h3>候选内容</h3>
            <p>{candidate.content}</p>
            <div className="actions">
              <button onClick={adoptCandidate}>采纳</button>
              <button onClick={rejectCandidate}>拒绝</button>
            </div>
          </>
        ) : (
          <p>{generating ? "AI 正在创作..." : "AI 生成后在此确认"}</p>
        )}
      </aside>
    </main>
  );
}

export default function App() {
  const selectedTab = useAppStore((state) => state.selectedTab);
  return selectedTab === "welcome" ? <Welcome /> : <Editor />;
}
